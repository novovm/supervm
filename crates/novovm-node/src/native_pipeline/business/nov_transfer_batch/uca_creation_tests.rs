//! Real signed account opening on the same seeded-tree execution fixtures.
use super::*;
use crate::native_pipeline::business::uca_spend_state::{
    creation_declarations, CapturedAssetSpendStateV2,
};
use novovm_protocol::uca_spend::{
    AssetAccountCreationV2, AssetAuthorityUpdateV2, AssetGrantConfigV2,
};

struct CreationRunner<'a> {
    facade: &'a AoemExecFacade,
    session: ComputeSession,
    integer: Option<ResidentTransferResources>,
    policy: DirectNovFeePolicy,
}

impl CreationRunner<'_> {
    fn run(
        &mut self,
        memory: &mut Memory,
        root: &mut NodeHash,
        intents: &[AssetSpendIntentV2],
        keys: &[(Vec<u8>, Vec<u8>)],
    ) -> Result<super::super::execution::ExecutedNovBatch> {
        let signer = self.facade.asset_spend_signer_v2()?;
        let verifier = self.facade.asset_spend_verifier_v2()?;
        let signed = intents
            .iter()
            .map(|intent| {
                let key = keys
                    .iter()
                    .find(|(public, _)| spend_key_ref_v1(public).ok() == Some(intent.spend_key_ref))
                    .context("opening fixture signer missing")?;
                signer.sign(intent.clone(), &key.0, &key.1)
            })
            .collect::<Result<Vec<_>>>()?;
        let capture = || {
            NovTransferInput::capture_uca(
                signed
                    .iter()
                    .cloned()
                    .map(|row| verifier.verify(row))
                    .collect::<Result<Vec<_>>>()?,
                context(*root, &self.policy)?,
                self.policy.clone(),
                memory,
                PlanBudget {
                    transactions: 8,
                    transaction_bytes: 8192,
                    body_bytes: 65_536,
                    access_keys: 4096,
                },
                capture_budget(),
            )
        };
        let cpu = capture()?.execute(&mut self.session, Duration::from_secs(30))?;
        if let Some(integer) = &mut self.integer {
            let gpu = capture()?.execute_integer_plan(integer)?;
            assert_eq!(cpu.receipts(), gpu.receipts(), "opening money/fees/nonce");
            assert_eq!(cpu.spend_outputs(), gpu.spend_outputs(), "opening budgets");
            assert_eq!(cpu.fees(), gpu.fees(), "opening complete fee state");
            assert_eq!(cpu.effects().update().root(), gpu.effects().update().root());
            assert_eq!(
                cpu.effects().update().nodes(),
                gpu.effects().update().nodes()
            );
            assert_eq!(
                cpu.receipt_batch_commitment(),
                gpu.receipt_batch_commitment()
            );
            assert_eq!(cpu.statement_commitment(), gpu.statement_commitment());
            assert_eq!(gpu.observation().gpu_waves, 1);
            assert_eq!(gpu.observation().recomputed_transactions, 0);
        }
        *root = cpu.effects().update().root();
        memory.0.extend(cpu.effects().update().nodes().clone());
        Ok(cpu)
    }
}

fn read_state(
    memory: &Memory,
    root: NodeHash,
    intent: &AssetSpendIntentV2,
) -> Result<CapturedAssetSpendStateV2> {
    let access = AssetSpendStateAccessV2::new(
        intent.clone(),
        Account::try_from(intent.payer.as_slice()).map_err(anyhow::Error::msg)?,
        Account::try_from(intent.recipient.as_slice()).map_err(anyhow::Error::msg)?,
    )?;
    let captured = OwnedStateInput::capture(memory, root, access.declarations(), capture_budget())?;
    access.read(&captured, root)
}

pub(super) fn run_asset_creation_cases(
    path: &std::path::Path,
    facade: &AoemExecFacade,
    compare_integer: bool,
) -> Result<()> {
    let mut keys = (0..6)
        .map(|_| {
            aoem_bindings::mldsa_keygen_v1_auto(65)?
                .context("account opening real ML-DSA fixture key unavailable")
        })
        .collect::<Result<Vec<_>>>()?;
    let result = (|| -> Result<()> {
        let refs = keys
            .iter()
            .map(|(public, _)| spend_key_ref_v1(public))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let descriptor = AssetAccountCreationV2 {
            management_key_ref: refs[1],
            recovery_key_ref: Some(refs[3]),
            grants: vec![AssetGrantConfigV2 {
                key_ref: refs[2],
                permits_nov_transfer: true,
                not_before_height: 1,
                expires_at_height: 30,
                max_debit_per_intent: 500,
                remaining_debit_budget: 1000,
            }],
        };
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
            recipient_creation: None,
        };
        let policy = super::super::body_tests::policy();
        let (mut memory, mut root) = fixture(&base, 5000, &policy, "opening")?;
        let mut funding_grant = grant(&base, 5000, "opening");
        funding_grant.max_debit_per_intent = 1000;
        let staged =
            stage_state_update(&memory, root, &grant_record_changes(&base, &funding_grant)?)?;
        root = staged.root();
        memory.0.extend(staged.nodes().clone());
        let original_memory = Memory(memory.0.clone());
        let original_root = root;
        let mut opening = base.clone();
        opening.recipient = descriptor.account(7, &base.genesis)?.to_vec();
        opening.amount = 200;
        opening.recipient_creation = Some(descriptor.clone());
        let mut ordinary_missing = opening.clone();
        ordinary_missing.recipient_creation = None;
        assert!(read_state(&memory, root, &ordinary_missing).is_err());
        assert!(read_state(&memory, root, &opening)?.creates_recipient());

        // Each orphan leaf must reject the whole opening, not silently reset it.
        for declaration in creation_declarations(&opening)? {
            let orphan = stage_state_update(
                &original_memory,
                original_root,
                &[StateChange::Put {
                    key: declaration.key,
                    value: vec![1],
                }],
            )?;
            let mut orphan_memory = Memory(original_memory.0.clone());
            orphan_memory.0.extend(orphan.nodes().clone());
            assert!(read_state(&orphan_memory, orphan.root(), &opening).is_err());
        }
        let mut runner = CreationRunner {
            facade,
            session: ComputeSession::open(path, 4)?,
            integer: if compare_integer {
                Some(ResidentTransferResources::new(path, 4, 8)?)
            } else {
                None
            },
            policy,
        };
        for revoked in [true, false] {
            let mut denied_grant = funding_grant.clone();
            denied_grant.revoked = revoked;
            if !revoked {
                denied_grant.remaining_debit_budget = opening.amount - 1;
            }
            let mut denied_memory = Memory(original_memory.0.clone());
            let staged = stage_state_update(
                &denied_memory,
                original_root,
                &grant_record_changes(&base, &denied_grant)?,
            )?;
            let mut denied_root = staged.root();
            denied_memory.0.extend(staged.nodes().clone());
            let rejected = runner.run(
                &mut denied_memory,
                &mut denied_root,
                &[opening.clone()],
                &keys,
            )?;
            assert_eq!(
                rejected.spend_outputs().unwrap()[0].failure,
                Some(if revoked {
                    SpendPolicyError::Revoked
                } else {
                    SpendPolicyError::LimitExceeded
                })
            );
            assert_eq!(rejected.receipts()[0].delta.fee_funding_delta, 0);
            assert_eq!(
                read_state(&denied_memory, denied_root, &base)?.next_nonce(),
                0
            );
            for declaration in creation_declarations(&opening)? {
                assert!(crate::native_pipeline::state::tree::read_state_value(
                    &denied_memory,
                    denied_root,
                    &declaration.key
                )?
                .is_none());
            }
        }
        let mut duplicate = opening.clone();
        duplicate.nonce = 1;
        let created = runner.run(&mut memory, &mut root, &[opening.clone(), duplicate], &keys)?;
        assert!(created
            .receipts()
            .iter()
            .all(|receipt| receipt.failure.is_none()));
        assert_eq!(created.receipts()[0].delta.recipient.before, 0);
        assert_eq!(created.receipts()[1].delta.recipient.before, 200);
        let mut spend = base.clone();
        spend.payer = opening.recipient.clone();
        spend.authorization_epoch = 1;
        spend.spend_key_ref = refs[2];
        spend.amount = 10;
        let state = read_state(&memory, root, &spend)?;
        assert_eq!(state.balances(), (400, 7));
        assert_eq!(state.next_nonce(), 0);
        assert_eq!(state.payer().management_key_ref, refs[1]);
        assert_eq!(state.payer().recovery_key_ref, Some(refs[3]));
        assert_eq!(state.grant().unwrap().remaining_debit_budget, 1000);
        let funding_state = read_state(&memory, root, &base)?;
        let opening_fees: u128 = created
            .receipts()
            .iter()
            .map(|r| r.delta.fee_funding_delta)
            .sum();
        assert_eq!(funding_state.balances().0, 1000 - 400 - opening_fees);
        assert_eq!(funding_state.next_nonce(), 2);
        assert_eq!(
            funding_state.grant().unwrap().remaining_debit_budget,
            5000 - 400 - opening_fees
        );

        let spent = runner.run(&mut memory, &mut root, &[spend.clone()], &keys)?;
        assert!(
            spent.receipts()[0].failure.is_none(),
            "credited NOV must be spendable"
        );
        let spent_fee = spent.receipts()[0].delta.fee_funding_delta;
        let state = read_state(&memory, root, &spend)?;
        assert_eq!(state.next_nonce(), 1);
        assert_eq!(state.balances(), (400 - 10 - spent_fee, 17));
        assert_eq!(
            state.grant().unwrap().remaining_debit_budget,
            1000 - 10 - spent_fee
        );

        let mut refill = opening.clone();
        refill.nonce = 2;
        refill.amount = 100;
        assert!(runner
            .run(&mut memory, &mut root, &[refill.clone()], &keys)?
            .receipts()[0]
            .failure
            .is_none());
        let state = read_state(&memory, root, &spend)?;
        assert_eq!(
            state.next_nonce(),
            1,
            "incoming opening hint must not reset nonce"
        );
        assert_eq!(
            state.grant().unwrap().remaining_debit_budget,
            1000 - 10 - spent_fee
        );

        let mut rotation = spend.clone();
        rotation.amount = 0;
        rotation.recipient = rotation.payer.clone();
        rotation.spend_key_ref = refs[1];
        rotation.nonce = 1;
        rotation.authority_update = Some(AssetAuthorityUpdateV2 {
            use_recovery: false,
            next_epoch: 2,
            active: true,
            management_key_ref: refs[4],
            recovery_key_ref: Some(refs[3]),
            grants: vec![AssetGrantConfigV2 {
                key_ref: refs[5],
                permits_nov_transfer: true,
                not_before_height: 1,
                expires_at_height: 30,
                max_debit_per_intent: 500,
                remaining_debit_budget: 700,
            }],
        });
        assert!(runner
            .run(&mut memory, &mut root, &[rotation], &keys)?
            .receipts()[0]
            .failure
            .is_none());
        refill.nonce = 3;
        refill.amount = 10;
        assert!(runner
            .run(&mut memory, &mut root, &[refill], &keys)?
            .receipts()[0]
            .failure
            .is_none());
        let mut rotated_spend = spend.clone();
        rotated_spend.authorization_epoch = 2;
        rotated_spend.spend_key_ref = refs[5];
        rotated_spend.nonce = 2;
        let state = read_state(&memory, root, &rotated_spend)?;
        assert_eq!(state.payer().management_key_ref, refs[4]);
        assert_eq!(state.payer().current_spend_epoch, 2);
        assert_eq!(state.next_nonce(), 2);
        assert_eq!(state.grant().unwrap().remaining_debit_budget, 700);
        assert!(runner
            .run(&mut memory, &mut root, &[rotated_spend.clone()], &keys)?
            .receipts()[0]
            .failure
            .is_none());
        let mut stale = spend;
        stale.nonce = 3;
        let rejected = runner.run(&mut memory, &mut root, &[stale], &keys)?;
        assert_eq!(
            rejected.spend_outputs().unwrap()[0].failure,
            Some(SpendPolicyError::WrongEpoch)
        );
        assert_eq!(rejected.receipts()[0].delta.nonce_after, 3);
        assert_eq!(rejected.receipts()[0].delta.fee_funding_delta, 0);

        let mut failed_opening = opening;
        failed_opening.nonce = 4;
        failed_opening.max_fee = 0;
        let descriptor = failed_opening.recipient_creation.as_mut().unwrap();
        descriptor.management_key_ref = refs[4];
        failed_opening.recipient = descriptor.account(7, &base.genesis)?.to_vec();
        let rejected = runner.run(&mut memory, &mut root, &[failed_opening.clone()], &keys)?;
        assert!(rejected.receipts()[0].failure.is_some());
        assert_eq!(rejected.receipts()[0].delta.fee_funding_delta, 0);
        for declaration in creation_declarations(&failed_opening)? {
            assert!(crate::native_pipeline::state::tree::read_state_value(
                &memory,
                root,
                &declaration.key
            )?
            .is_none());
        }
        assert!(read_state(&memory, root, &failed_opening)?.creates_recipient());
        Ok(())
    })();
    for (_, secret) in &mut keys {
        secret.fill(0);
    }
    result
}
