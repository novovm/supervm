//! Real ML-DSA NUAS input through the ORIGINAL socket/lifecycle/pipeline/journal.
//! This explicit seeded-state fixture does not authorize genesis or deployment.
//! Validator signing uses the existing branded consensus component fixture;
//! four sockets/stores in this process are not independent-process/TPS evidence.
use super::*;
use crate::native_pipeline::business::nov_transfer_batch::{
    nonce_key, uca_effect_contract, uca_program_id, uca_receipt_codec,
};
use crate::native_pipeline::business::uca_spend_state::{
    account_record_changes, grant_record_changes, AssetAccountAuthorityV2, AssetSpendStateAccessV2,
};
use crate::native_pipeline::pipeline::admitted_uca_for_test;
use crate::native_pipeline::state::frontier::OwnedStateInput;
use novovm_adapter_api::uca_spend_policy::{SpendPolicyError, SuppliedAssetSpendGrantV2};
use novovm_exec::pq_signature::AoemMldsaParameterSet;
use novovm_exec::AoemExecFacade;
use novovm_protocol::uca_spend::{spend_key_ref_v1, AssetSpendIntentV2, MAX_SIGNED_SPEND_BYTES};
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV2, AssetSpendBatchV2};

const ALICE: &str = "uca:successor-alice";
const BOB: &str = "uca:successor-bob";

fn uca_codec() -> transport::DecodeLimits {
    transport::DecodeLimits {
        transactions: 8,
        transaction_bytes: MAX_SIGNED_SPEND_BYTES,
        body_bytes: 64 * 1024,
        message_bytes: 128 * 1024,
    }
}

fn configure(path: &Path) -> Result<PipelineConfig> {
    let mut config = pipeline_config(path)?;
    config.authentication.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    config.authentication.body_bytes = uca_codec().body_bytes;
    config.plan.transaction_bytes = MAX_SIGNED_SPEND_BYTES;
    config.plan.body_bytes = uca_codec().body_bytes;
    config.plan.access_keys = 512;
    config.capture.keys = 512;
    Ok(config)
}

fn address(id: &str) -> Account {
    Account::from(if id == ALICE { [41; 20] } else { [42; 20] })
}

fn account_state(id: &str) -> AssetAccountAuthorityV2 {
    AssetAccountAuthorityV2 {
        account: address(id).as_bytes().to_vec(),
        asset: 1,
        active: true,
        current_spend_epoch: 1,
        management_key_ref: [8; 32],
        recovery_key_ref: None,
    }
}

fn intent(
    public: &[u8],
    payer: &str,
    to: &str,
    nonce: u64,
    amount: u128,
) -> Result<AssetSpendIntentV2> {
    Ok(AssetSpendIntentV2 {
        asset: 1,
        authority_update: None,
        chain_id: CHAIN,
        genesis: GENESIS,
        payer: address(payer).as_bytes().to_vec(),
        authorization_epoch: 1,
        spend_key_ref: spend_key_ref_v1(public)?,
        recipient: address(to).as_bytes().to_vec(),
        amount,
        max_fee: 50,
        nonce,
        not_before_height: 1,
        expires_at_height: 30,
    })
}

fn uca_context(parent: ParentPoint, height: u64) -> Result<BatchContext> {
    let mut context = batch_context(parent.state_root);
    context.business_program = uca_program_id();
    context.receipt_codec = uca_receipt_codec();
    context.effect_contract = uca_effect_contract(&policy())?;
    context.parent_height = parent.height;
    context.parent_block_hash = parent.block_hash;
    context.parent_receipt_root = parent.receipt_batch_commitment;
    context.parent_state_version = parent.state_version;
    context.height = height;
    context.slot = height - 1;
    context.timestamp_unix_ms += height;
    Ok(context)
}

struct UcaFixture {
    native: NativeFixture,
    parent_batch: Arc<AssetSpendBatchV2>,
    child_batch: Arc<AssetSpendBatchV2>,
    grants: Vec<AssetSpendIntentV2>,
    facade: AoemExecFacade,
}

impl UcaFixture {
    fn new() -> Result<Self> {
        let facade = AoemExecFacade::open(&library()?, Default::default())?;
        let signer = facade.asset_spend_signer_v2()?;
        let mut keys = (0..4)
            .map(|_| facade.mldsa_keygen_v1(AoemMldsaParameterSet::MlDsa65))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let grants = vec![
            intent(&keys[0].public_key, ALICE, BOB, 0, 200)?,
            intent(&keys[1].public_key, ALICE, BOB, 1, 10)?,
            intent(&keys[2].public_key, BOB, ALICE, 0, 1000)?,
            intent(&keys[3].public_key, ALICE, BOB, 3, 1)?,
        ];
        let mut changes = fee_record_changes(&policy(), &FeeState::default())?;
        for (id, balance) in [(ALICE, 1000u128), (BOB, 0)] {
            changes.extend(account_record_changes(CHAIN, &GENESIS, &account_state(id))?);
            changes.push(StateChange::Put {
                key: balance_key(&address(id)),
                value: balance.to_le_bytes().to_vec(),
            });
        }
        for (index, grant) in grants.iter().enumerate() {
            changes.extend(grant_record_changes(
                grant,
                &SuppliedAssetSpendGrantV2 {
                    asset: 1,
                    account: grant.payer.clone(),
                    key_ref: grant.spend_key_ref,
                    authorization_epoch: 1,
                    permits_nov_transfer: true,
                    revoked: index == 3,
                    not_before_height: 1,
                    expires_at_height: 30,
                    max_debit_per_intent: 2000,
                    remaining_debit_budget: [350, 500, 2000, 500][index],
                },
            )?);
        }
        for index in [0, 2] {
            changes.push(StateChange::Put {
                key: nonce_key(&grants[index].nonce_identity()?),
                value: 0u64.to_le_bytes().to_vec(),
            });
        }
        let make_batch = |rows: &[(usize, AssetSpendIntentV2)]| -> Result<Arc<AssetSpendBatchV2>> {
            let mut raw = Vec::new();
            for (key, intent) in rows {
                raw.push(
                    signer
                        .sign(
                            intent.clone(),
                            &keys[*key].public_key,
                            &keys[*key].expanded_secret_key,
                        )?
                        .encode()?,
                );
            }
            Ok(Arc::new(AssetSpendBatchV2::from_raw(
                &raw,
                AssetSpendBatchLimitsV2 {
                    transactions: uca_codec().transactions,
                    transaction_bytes: uca_codec().transaction_bytes,
                    body_bytes: uca_codec().body_bytes,
                },
            )?))
        };
        let parent_batch = make_batch(&[(0, grants[0].clone())])?;
        let mut exhausted = grants[0].clone();
        exhausted.nonce = 1;
        let mut self_transfer = grants[1].clone();
        self_transfer.nonce = 2;
        self_transfer.amount = 5;
        self_transfer.recipient = address(ALICE).as_bytes().to_vec();
        let mut respend = grants[2].clone();
        respend.nonce = 1;
        respend.amount = 20;
        let mut replay = grants[1].clone();
        replay.nonce = 2;
        replay.amount = 1;
        let child_batch = make_batch(&[
            (0, exhausted),
            (1, grants[1].clone()),
            (1, self_transfer),
            (2, grants[2].clone()),
            (2, respend),
            (1, replay),
            (3, grants[3].clone()),
        ])?;
        for key in &mut keys {
            key.expanded_secret_key.fill(0);
        }
        let mut base = fixture_with(
            ExecutionBackend::CpuCallbacks,
            uca_codec(),
            |path| {
                fs::create_dir_all(path)?;
                let staged = stage_state_update(&Memory::default(), empty_root(), &changes)?;
                let store = CandidateStore::open(configure(path)?.store, OpenMode::CreateNew)?;
                store.install_unpublished_state(&staged)?;
                Ok(staged.root())
            },
            configure,
        )?;
        let mut nodes = Vec::new();
        for (i, (driver, channel)) in std::mem::take(&mut base.drivers)
            .into_iter()
            .zip(std::mem::take(&mut base.mesh.channels))
            .enumerate()
        {
            let bodies = CandidateBodies::new(
                &base.services.0[i],
                uca_context(base.anchor, 1)?,
                policy(),
                uca_codec(),
                BodyLimits {
                    bodies: 8,
                    bytes: 64 * 1024 * 1024,
                    inflight: 2,
                },
            )?;
            nodes.push(NativeLifecycle::new(
                driver,
                bodies,
                channel,
                &base.set,
                DeliveryLimits {
                    messages_per_peer: 32,
                    bytes_per_peer: 1024 * 1024,
                },
                policy(),
                uca_codec(),
            )?);
        }
        Ok(Self {
            native: NativeFixture {
                nodes,
                base,
                started: Instant::now(),
            },
            parent_batch,
            child_batch,
            grants,
            facade,
        })
    }

    fn prepare_successor(
        &mut self,
        reused: bool,
    ) -> Result<(usize, usize, SuccessorSelection, Arc<Message>)> {
        let f = &mut self.native;
        let first = f.leader(1)?;
        let next = f.leader(2)?;
        let parent = Arc::new(Message::AssetBody {
            context: uca_context(f.base.anchor, 1)?,
            batch: self.parent_batch.clone(),
        });
        ensure!(f.nodes[first].offer(&parent)?, "UCA parent backpressured");
        f.until(&[first, next], "UCA parent executed before ACK", |nodes| {
            nodes[next].executed() == 1
        })?;
        let selection = f.nodes[next]
            .successor_selection()?
            .context("UCA successor basis absent")?;
        ensure!(
            selection.nonce_after.get(&self.grants[0].nonce_identity()?) == Some(&1),
            "real UCA parent receipt missing stable account nonce"
        );
        let child = Arc::new(Message::AssetBody {
            context: uca_context(selection.parent, 2)?,
            batch: self.child_batch.clone(),
        });
        reject_altered_pins(
            &mut f.nodes[next],
            &f.base.services.0[next],
            &child,
            &selection,
        )?;
        ensure!(
            f.nodes[next]
                .offer_successor(
                    &body(selection.parent, 2)?,
                    &selection,
                    &f.base.services.0[next]
                )
                .is_err(),
            "V3 successor bypassed UCA profile"
        );
        let accepted = if reused {
            let pipeline = &f.base.services.0[next];
            let admitted = (0..self.child_batch.len())
                .map(|index| {
                    admitted_uca_for_test(
                        pipeline.owner_identity(),
                        &self.facade,
                        self.child_batch.clone(),
                        index,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            f.nodes[next].offer_successor_admitted(
                &child,
                &selection,
                pipeline,
                &admitted.iter().collect::<Vec<_>>(),
            )?
        } else {
            f.nodes[next].offer_successor(&child, &selection, &f.base.services.0[next])?
        };
        ensure!(accepted, "typed UCA successor backpressured");
        Ok((first, next, selection, child))
    }
}

#[test]
#[ignore = "real AOEM and native sockets; full UCA speculative root, ACK, credits and cold recovery, not TPS"]
fn native_successor_uca_complete_state_waits_for_ack_and_recovers() -> Result<()> {
    run_uca_successor(false)
}

#[test]
#[ignore = "real AOEM and native sockets; reused signature facts recapture full UCA successor state"]
fn native_successor_uca_reused_signatures_keep_budget_nonce_fee_and_recovery() -> Result<()> {
    run_uca_successor(true)
}

fn run_uca_successor(reused: bool) -> Result<()> {
    let mut u = UcaFixture::new()?;
    let (first, next, selection, child) = u.prepare_successor(reused)?;
    let f = &mut u.native;
    let child_id = transport::candidate_body_id(&child, uca_codec())?;
    f.until(
        &[first, next],
        "full UCA successor persisted before parent ACK",
        |nodes| nodes[next].successor_metrics()[2] == 1,
    )?;
    ensure!(
        f.nodes[next].successor_metrics() == [1, 1, 1, 0, 0],
        "premature UCA promotion"
    );
    ensure!(
        f.nodes.iter().all(|n| n.progress.head().is_none())
            && !f.nodes[next].progress.retains_body(child_id),
        "speculation acquired finality"
    );
    let journal = open_journal(
        ValidatorJournal::open(
            &f.base.services.0[next],
            f.base.genesis,
            f.base.anchor,
            f.base.set.clone(),
            f.base.keys[next].clone(),
        )?,
        &f.base.services.0[next],
    )?;
    ensure!(
        journal.head().is_none() && journal.context().height == 1,
        "private UCA root published"
    );
    drop(journal);
    let final_head = f.await_height(2)?;
    ensure!(
        f.nodes[next].successor_metrics() == [1, 1, 1, 1, 0],
        "UCA promotion was not exact once"
    );
    ensure!(
        f.nodes.iter().all(|n| n.executed() == 2),
        "UCA child re-executed or bypassed"
    );
    drop(std::mem::take(&mut f.nodes));
    f.base.services.shutdown()?;
    let mut expected_receipts = None;
    let mut expected_outputs = None;
    for (i, path) in f.base.paths.iter().enumerate() {
        let p = CandidatePipeline::start(configure(path)?, OpenMode::Existing)?;
        let journal = open_journal(
            ValidatorJournal::open(
                &p,
                f.base.genesis,
                f.base.anchor,
                f.base.set.clone(),
                f.base.keys[i].clone(),
            )?,
            &p,
        )?;
        ensure!(
            journal.head() == Some(final_head),
            "UCA successor cold head differs"
        );
        drop(journal);
        let mut read = ArchiveRead::new(2, final_head, f.base.genesis, f.base.set.clone())?;
        let deadline = Instant::now() + DEADLINE;
        let archive = loop {
            ensure!(Instant::now() < deadline, "UCA successor archive deadline");
            if let Some(result) = read.poll(&p)? {
                break result.context("missing UCA child archive")?;
            }
            std::thread::yield_now();
        };
        ensure!(archive.point() == final_head, "UCA archive point differs");
        let stored = archive.stored();
        let outputs = stored.spend_outputs().context("UCA grant outputs absent")?;
        let receipts = (0..7)
            .map(|n| stored.receipt_view(n))
            .collect::<Result<Vec<_>>>()?;
        ensure!(outputs.len() == 7, "UCA successor output count");
        ensure!(
            outputs[0].failure == Some(SpendPolicyError::LimitExceeded)
                && receipts[0].nonce_after == 1
                && receipts[0].charged_fee == "0",
            "successor reused pre-parent grant budget or charged rejected authorization"
        );
        ensure!(
            receipts[1].success
                && receipts[1].nonce_after == 2
                && receipts[2].success
                && receipts[2].nonce_after == 3,
            "second grant/self transfer did not share stable UCA nonce"
        );
        let fees = receipts
            .iter()
            .map(|r| r.charged_fee.parse::<u128>().map_err(anyhow::Error::from))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            !receipts[3].success
                && outputs[3].failure.is_none()
                && receipts[3].nonce_after == 1
                && fees[3] > 0
                && outputs[3].remaining == 2000 - fees[3],
            "business failure did not charge only actual fee"
        );
        ensure!(
            receipts[4].success && receipts[4].nonce_after == 2,
            "parent's credited NOV was not spendable by its recipient"
        );
        ensure!(
            outputs[5].failure == Some(SpendPolicyError::NonceMismatch)
                && outputs[6].failure == Some(SpendPolicyError::Revoked)
                && receipts[5].nonce_after == 3
                && receipts[6].nonce_after == 3
                && fees[5] == 0
                && fees[6] == 0,
            "replay/revoked key bypassed fresh UCA authority"
        );
        ensure!(
            outputs[1].remaining == 500 - 10 - fees[1]
                && outputs[2].remaining == 500 - 10 - fees[1] - 5 - fees[2],
            "self transfer did not consume gross amount plus actual fee"
        );
        if let Some(expected) = &expected_receipts {
            ensure!(
                stored.receipt_bytes() == expected,
                "cold receipt bytes differ"
            );
        } else {
            expected_receipts = Some(stored.receipt_bytes().to_vec());
        }
        if let Some(expected) = &expected_outputs {
            ensure!(outputs == expected, "cold UCA output bytes differ");
        } else {
            expected_outputs = Some(outputs.to_vec());
        }
        p.shutdown()?;
        let store = CandidateStore::open(configure(path)?.store, OpenMode::Existing)?;
        let expected = [
            outputs[0].remaining,
            outputs[2].remaining,
            outputs[4].remaining,
            outputs[6].remaining,
        ];
        for (index, grant) in u.grants.iter().enumerate() {
            let access = AssetSpendStateAccessV2::new(
                grant.clone(),
                Account::try_from(grant.payer.clone()).map_err(anyhow::Error::msg)?,
                Account::try_from(grant.recipient.clone()).map_err(anyhow::Error::msg)?,
            )?;
            let state = OwnedStateInput::capture(
                &store,
                final_head.state_root,
                access.declarations(),
                configure(path)?.capture,
            )?;
            let state = access.read(&state, final_head.state_root)?;
            ensure!(
                state
                    .grant()
                    .context("cold grant missing")?
                    .remaining_debit_budget
                    == expected[index],
                "cold grant budget differs"
            );
            ensure!(
                state.next_nonce() == if index == 2 { 2 } else { 3 },
                "cold stable UCA nonce differs"
            );
            if index == 0 {
                let parent_fee = 350 - 200 - outputs[0].remaining;
                let (alice, bob) = state.balances();
                ensure!(
                    alice == 1000 - 200 - parent_fee - 10 - fees[1] - fees[2] + 20
                        && bob == 200 + 10 - fees[3] - 20 - fees[4]
                        && alice + bob + parent_fee + fees.iter().sum::<u128>() == 1000,
                    "cold balances and actual fees are not conserved"
                );
            }
        }
        // Parent content remains separately addressable, not overwritten by the
        // child's private execution. Journal publication alone chose final_head.
        ensure!(
            selection.parent.height == 1 && selection.parent.state_root != final_head.state_root,
            "successor did not preserve distinct immutable parent state"
        );
    }
    Ok(())
}

#[test]
#[ignore = "real AOEM/native UCA; original background auth/execution tickets drain after durable round change"]
fn native_successor_uca_round_change_drains_auth_and_execution_without_publication() -> Result<()> {
    for execution_started in [false, true] {
        let mut u = UcaFixture::new()?;
        let (first, next, selection, child) = u.prepare_successor(true)?;
        let f = &mut u.native;
        let child_id = transport::candidate_body_id(&child, uca_codec())?;
        let counter = usize::from(execution_started);
        f.until(&[first, next], "accepted UCA successor ticket", |nodes| {
            nodes[next].successor_metrics()[counter] == 1
        })?;
        // Freeze assembly polling at an actually accepted ticket. Only signed
        // votes from the existing component fixture request the durable round
        // transition; no QC, parent ACK or journal record is fabricated here.
        let new_round = selection.source_round + 1;
        for peer in (0..4).filter(|i| *i != next).take(2) {
            let vote = novovm_consensus::round_bft::test_vectors::sign_vote(
                selection.source,
                new_round,
                crate::native_pipeline::consensus::wire::Phase::Prevote,
                None,
                &f.base.set,
                &f.base.keys[peer],
            )?;
            f.nodes[next]
                .progress
                .receive(f.base.ids[peer], &Message::Vote(vote))?;
        }
        let deadline = Instant::now() + DEADLINE;
        let mut held = Vec::new();
        while f.nodes[next].progress.round() != new_round {
            ensure!(Instant::now() < deadline, "UCA source round deadline");
            if let Some(message) = f.nodes[next].progress.take_message() {
                held.push(message);
            }
            f.nodes[next]
                .progress
                .poll(&f.base.services.0[next], Instant::now())?;
            std::thread::yield_now();
        }
        let mut probe = Some(
            crate::native_pipeline::pipeline::AuthenticationRequest::from_uca_batch(
                u.child_batch.clone(),
                policy(),
            )?,
        );
        let mut ticket = loop {
            ensure!(
                Instant::now() < deadline,
                "retired UCA successor permit was not drained"
            );
            f.nodes[next].poll(&f.base.services.0[next], Instant::now())?;
            match f.base.services.0[next].try_authenticate_owned(probe.take().unwrap()) {
                Ok(crate::native_pipeline::pipeline::AuthenticationSubmission::Accepted(
                    ticket,
                )) => break ticket,
                Ok(crate::native_pipeline::pipeline::AuthenticationSubmission::Backpressured(
                    request,
                )) => probe = Some(request),
                Err(rejected) => return Err(rejected.error),
            }
            std::thread::yield_now();
        };
        loop {
            ensure!(Instant::now() < deadline, "UCA drain probe auth deadline");
            if let Some(body) = ticket.try_take()? {
                drop(body);
                break;
            }
            std::thread::yield_now();
        }
        ensure!(
            f.nodes[next].successor_metrics()
                == if execution_started {
                    [1, 1, 0, 0, 1]
                } else {
                    [1, 0, 0, 0, 1]
                },
            "retired UCA completion revived a target"
        );
        ensure!(
            f.nodes[next].progress.head().is_none()
                && !f.nodes[next].progress.retains_body(child_id)
                && !f.nodes[next].has_successor_target(2),
            "discarded UCA successor gained authority"
        );
        let journal = open_journal(
            ValidatorJournal::open(
                &f.base.services.0[next],
                f.base.genesis,
                f.base.anchor,
                f.base.set.clone(),
                f.base.keys[next].clone(),
            )?,
            &f.base.services.0[next],
        )?;
        ensure!(
            journal.head().is_none() && journal.round() == new_round,
            "round drain lost durable safety state"
        );
        drop(journal);
        drop(held);
        drop(std::mem::take(&mut f.nodes));
        f.base.services.shutdown()?;
    }
    Ok(())
}
