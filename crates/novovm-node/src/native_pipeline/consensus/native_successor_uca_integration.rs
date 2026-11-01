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
use novovm_protocol::uca_spend::{spend_key_ref_v1, AssetSpendIntentV3, MAX_SIGNED_SPEND_BYTES};
use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3};

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
) -> Result<AssetSpendIntentV3> {
    Ok(AssetSpendIntentV3 {
        authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
        asset: 1,
        authority_update: None,
        recipient_creation: None,
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
    parent_batch: Arc<AssetSpendBatchV3>,
    child_batch: Arc<AssetSpendBatchV3>,
    grants: Vec<AssetSpendIntentV3>,
    facade: AoemExecFacade,
}

impl UcaFixture {
    fn new() -> Result<Self> {
        let facade = AoemExecFacade::open(&library()?, Default::default())?;
        let signer = facade.asset_spend_signer_v3()?;
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
        let make_batch = |rows: &[(usize, AssetSpendIntentV3)]| -> Result<Arc<AssetSpendBatchV3>> {
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
            Ok(Arc::new(AssetSpendBatchV3::from_raw(
                &raw,
                AssetSpendBatchLimitsV3 {
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
        for _ in 0..8 {
            let repeated = f.nodes[next]
                .successor_selection()?
                .context("unchanged UCA successor basis disappeared")?;
            ensure!(
                repeated.parent == selection.parent
                    && repeated.source == selection.source
                    && repeated.source_round == selection.source_round
                    && Arc::ptr_eq(&repeated.nonce_after, &selection.nonce_after),
                "unchanged executed basis rebuilt its nonce projection"
            );
        }
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
#[ignore = "real AOEM; local durable asset preparation overlaps the missing parent proposal without signing"]
fn native_successor_uca_local_ready_prepares_before_parent_proposal() -> Result<()> {
    run_local_ready_successor(false)
}

#[test]
#[ignore = "real AOEM/native sockets; one next-leader body prepares all followers before exact parent ACK"]
fn native_successor_uca_remote_body_waits_for_own_parent_without_blocking_current_body(
) -> Result<()> {
    let mut u = UcaFixture::new()?;
    let f = &mut u.native;
    let first = f.leader(1)?;
    let next = f.leader(2)?;
    ensure!(first != next);
    let parent = Arc::new(Message::AssetBody {
        context: uca_context(f.base.anchor, 1)?,
        batch: u.parent_batch.clone(),
    });
    ensure!(f.nodes[next].offer(&parent)?);
    f.until(&[next], "next leader own parent ready", |nodes| {
        nodes[next].executed() == 1
    })?;
    let selection = f.nodes[next]
        .successor_selection()?
        .context("missing local parent")?;
    let child = Arc::new(Message::AssetBody {
        context: uca_context(selection.parent, 2)?,
        batch: u.child_batch.clone(),
    });
    let child_id = transport::candidate_body_id(&child, uca_codec())?;
    ensure!(f.nodes[next].offer_successor(&child, &selection, &f.base.services.0[next])?);
    // No follower has the parent yet. Native AssetBody reception must transfer
    // its one optional future object to CandidateBodies, releasing pending[1].
    f.until(
        &[0, 1, 2, 3],
        "future body retained without trusting its parent",
        |nodes| {
            nodes
                .iter()
                .enumerate()
                .all(|(i, node)| i == next || node.remote_successor_waiting_for_test(child_id))
        },
    )?;
    ensure!(
        f.nodes
            .iter()
            .enumerate()
            .all(|(i, node)| i == next
                || (node.executed() == 0 && node.successor_metrics() == [0; 5]))
    );
    ensure!(f.nodes[next].successor_body_enqueued == 3);
    let Message::AssetBody { context, batch } = child.as_ref() else {
        anyhow::bail!("expected original asset successor");
    };
    let reference = transport::uca_body_reference(context, batch, uca_codec())?;
    ensure!(
        f.nodes[next].successor_body_bytes
            == 3 * transport::encode(&reference, uca_codec())?.len() as u64
    );
    // Reuse only this receiver's existing capability, even though decoding the
    // network body created a different dictionary Arc. Other receivers below
    // deliberately keep the ordinary strict authentication path.
    let reused = (0..4)
        .find(|i| *i != first && *i != next)
        .context("missing reuse follower")?;
    let input = f.nodes[reused]
        .remote_successor_authentication_input()
        .context("missing remote authentication input")?;
    let foreign = (0..u.child_batch.len())
        .map(|index| {
            admitted_uca_for_test(
                f.base.services.0[next].owner_identity(),
                &u.facade,
                u.child_batch.clone(),
                index,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(f.nodes[reused]
        .reuse_remote_successor_admitted(
            &input,
            &f.base.services.0[reused],
            &foreign.iter().collect::<Vec<_>>()
        )
        .is_err());
    let admitted = (0..u.child_batch.len())
        .map(|index| {
            admitted_uca_for_test(
                f.base.services.0[reused].owner_identity(),
                &u.facade,
                u.child_batch.clone(),
                index,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(f.nodes[reused].reuse_remote_successor_admitted(
        &input,
        &f.base.services.0[reused],
        &admitted.iter().collect::<Vec<_>>()
    )?);
    ensure!(f.nodes[reused]
        .remote_successor_authentication_input()
        .is_none());
    ensure!(!f.nodes[reused].reuse_remote_successor_admitted(
        &input,
        &f.base.services.0[reused],
        &admitted.iter().collect::<Vec<_>>()
    )?);
    // Each non-proposer uses its OWN parent execution; no child is locally
    // selected, copied from a database, or supplied by the test to followers.
    for i in 0..4 {
        if i != first && i != next {
            ensure!(f.nodes[i].offer(&parent)?);
        }
    }
    let followers = (0..4).filter(|i| *i != first).collect::<Vec<_>>();
    f.until(
        &followers,
        "three followers privately finish the same child",
        |nodes| {
            nodes
                .iter()
                .enumerate()
                .all(|(i, node)| i == first || node.successor_metrics()[2] == 1)
        },
    )?;
    ensure!(f
        .nodes
        .iter()
        .all(|node| node.progress.head().is_none() && !node.progress.retains_body(child_id)));
    ensure!(f.nodes[first].remote_successor_waiting_for_test(child_id));
    // Let the current proposer execute, but pause all voting peers. Its early
    // child must also complete without a quorum or a durable parent ACK.
    ensure!(f.nodes[first].offer(&parent)?);
    f.until(
        &[first],
        "current proposer privately finishes child before quorum",
        |nodes| nodes[first].successor_metrics()[2] == 1,
    )?;
    ensure!(f.nodes.iter().all(|node| node.progress.head().is_none()
        && !node.progress.retains_body(child_id)
        && node.successor_metrics() == [1, 1, 1, 0, 0]));
    for (i, node) in f.nodes.iter().enumerate() {
        let journal = open_journal(
            ValidatorJournal::open(
                &f.base.services.0[i],
                f.base.genesis,
                f.base.anchor,
                f.base.set.clone(),
                f.base.keys[i].clone(),
            )?,
            &f.base.services.0[i],
        )?;
        ensure!(journal.head().is_none() && journal.context().height == 1);
        ensure!(node.executed() == 2);
    }
    f.await_height(2)?;
    ensure!(f
        .nodes
        .iter()
        .all(|node| node.executed() == 2 && node.successor_metrics() == [1, 1, 1, 1, 0]));
    drop(std::mem::take(&mut f.nodes));
    f.base.services.shutdown()?;
    Ok(())
}

#[test]
#[ignore = "real AOEM; a different real parent decision discards the private local-ready successor"]
fn native_successor_uca_local_ready_discards_different_decided_parent() -> Result<()> {
    run_local_ready_successor(true)
}

#[derive(Clone, Copy)]
enum ReferenceInput {
    Exact,
    Missing,
    DifferentSignature,
}

#[test]
#[ignore = "real AOEM/UDP; exact local asset rows reconstruct the original body without full-body fallback"]
fn native_successor_uca_reference_reuses_exact_local_input() -> Result<()> {
    run_asset_reference(ReferenceInput::Exact)
}

#[test]
#[ignore = "real AOEM/UDP; missing local rows request the live successor body without blocking its lane"]
fn native_successor_uca_reference_missing_input_uses_live_body_reply() -> Result<()> {
    run_asset_reference(ReferenceInput::Missing)
}

#[test]
#[ignore = "real AOEM/UDP; intent hash hits with different signature bytes fall back to the original complete body"]
fn native_successor_uca_reference_signature_mismatch_uses_full_body() -> Result<()> {
    run_asset_reference(ReferenceInput::DifferentSignature)
}

#[test]
#[ignore = "real AOEM/UDP parent ACK; scheduled decoded full reply precedes the next leader's proposal"]
fn native_successor_uca_reference_full_reply_survives_parent_ack_before_proposal() -> Result<()> {
    let mut u = UcaFixture::new()?;
    let f = &mut u.native;
    let first = f.leader(1)?;
    let next = f.leader(2)?;
    let target = (0..4)
        .find(|i| *i != first && *i != next)
        .context("missing reference target")?;
    let parent = Arc::new(Message::AssetBody {
        context: uca_context(f.base.anchor, 1)?,
        batch: u.parent_batch.clone(),
    });
    ensure!(f.nodes[next].offer(&parent)?);
    f.until(&[next], "late reply sender parent basis", |nodes| {
        nodes[next].executed() == 1
    })?;
    let selection = f.nodes[next]
        .successor_selection()?
        .context("late reply parent basis missing")?;
    let child = Arc::new(Message::AssetBody {
        context: uca_context(selection.parent, 2)?,
        batch: u.child_batch.clone(),
    });
    let child_id = transport::candidate_body_id(&child, uca_codec())?;
    ensure!(f.nodes[next].offer_successor(&child, &selection, &f.base.services.0[next])?);
    f.nodes[next].poll(&f.base.services.0[next], Instant::now())?;
    ensure!(f.nodes[next].successor_body_enqueued == 3);

    // Stop only the next proposer's lifecycle. Its real socket still delivers
    // the reference and receives RequestBody, but cannot send a reply/proposal.
    let active = (0..4).filter(|i| *i != next).collect::<Vec<_>>();
    f.until(
        &active,
        "reference miss requested before parent ACK",
        |nodes| nodes[target].body_reference_full_requests == 1,
    )?;
    ensure!(f.nodes[next].served_bodies == 0);
    for (i, node) in f.nodes.iter_mut().enumerate() {
        if i != next {
            ensure!(node.offer(&parent)?);
        }
    }
    f.until(&active, "three real voters persist parent ACK", |nodes| {
        active.iter().all(|i| {
            nodes[*i].progress.head() == Some(selection.parent)
                && nodes[*i].progress.context().height == 2
        })
    })?;
    ensure!(f.nodes[next].progress.context().height == 1);
    ensure!(f.nodes[next].served_bodies == 0);
    ensure!(f.nodes[target].executed() == 1);
    ensure!(!f.nodes[target]
        .progress
        .missing_bodies()
        .any(|(_, id)| id == child_id));
    // Exercise the original periodic recovery turn without sleeping or moving
    // consensus time forward: it must not erase an unanswered reference miss.
    f.nodes[target].defer_archive_discovery_until_for_test(Instant::now());
    f.nodes[target].poll(&f.base.services.0[target], Instant::now())?;

    let requests = f.nodes[target].requested_bodies;
    let rejected = f.nodes[target].rejected_messages;
    // Schedule only the existing decoded body handoff, not a proposal, a
    // signature capability, a durable candidate or an ACK. The actual socket
    // path is covered by the adjacent missing-input/full-reply regression.
    f.nodes[target].retain_body_reply_for_test(f.base.ids[next], child.clone())?;
    for _ in 0..16 {
        f.nodes[target].poll(&f.base.services.0[target], Instant::now())?;
    }
    ensure!(
        f.nodes[target].rejected_messages == rejected,
        "requested full body was discarded after parent ACK"
    );
    ensure!(
        f.nodes[target].executed() == 1
            && !f.nodes[target].progress.retains_body(child_id)
            && f.nodes[target].progress.head() == Some(selection.parent),
        "requested bytes acquired execution or publication authority"
    );
    use crate::native_pipeline::consensus::journal::DurableMessage;
    ensure!(
        match f.nodes[target].progress.last_durable_message_for_test() {
            Some(DurableMessage::Proposal(proposal))
            | Some(DurableMessage::Decision { proposal, .. }) => proposal.context.height < 2,
            Some(DurableMessage::Vote(vote)) => vote.context.height < 2,
            None => true,
        },
        "body custody signed at the child height before its proposal"
    );

    let head = f.await_height(2)?;
    ensure!(f.nodes.iter().all(|node| node.executed() == 2));
    ensure!(f.nodes[target].progress.head() == Some(head));
    ensure!(
        f.nodes[target].requested_bodies == requests
            && f.nodes[target].body_reference_full_requests == 1,
        "held requested body was lost and fetched again after the proposal"
    );
    ensure!(!f.nodes[target].is_frozen());
    drop(std::mem::take(&mut f.nodes));
    f.base.services.shutdown()
}

fn run_asset_reference(input: ReferenceInput) -> Result<()> {
    let mut u = UcaFixture::new()?;
    let f = &mut u.native;
    let first = f.leader(1)?;
    let next = f.leader(2)?;
    ensure!(first != next);
    let parent = Arc::new(Message::AssetBody {
        context: uca_context(f.base.anchor, 1)?,
        batch: u.parent_batch.clone(),
    });
    ensure!(f.nodes[next].offer(&parent)?);
    f.until(&[next], "reference sender's own parent basis", |nodes| {
        nodes[next].executed() == 1
    })?;
    let selection = f.nodes[next]
        .successor_selection()?
        .context("reference sender has no actual parent basis")?;
    let context = uca_context(selection.parent, 2)?;
    let child = Arc::new(Message::AssetBody {
        context,
        batch: u.child_batch.clone(),
    });
    let child_id = transport::candidate_body_id(&child, uca_codec())?;
    let reference = transport::uca_body_reference(&context, &u.child_batch, uca_codec())?;
    let reference_bytes = transport::encode(&reference, uca_codec())?.len();
    let full_bytes = transport::encode(&child, uca_codec())?.len();
    ensure!(reference_bytes < full_bytes);

    // Fixture-local input is not a fabricated signature capability. The real
    // body owner must still authenticate it after reproducing its own parent.
    let local_input = match input {
        ReferenceInput::Exact => {
            let views = (0..u.child_batch.len())
                .map(|index| u.child_batch.view(index))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Some(Arc::new(AssetSpendBatchV3::from_views(
                &views,
                AssetSpendBatchLimitsV3 {
                    transactions: uca_codec().transactions,
                    transaction_bytes: uca_codec().transaction_bytes,
                    body_bytes: uca_codec().body_bytes,
                },
            )?))
        }
        ReferenceInput::Missing => None,
        ReferenceInput::DifferentSignature => {
            let mut rows = Vec::new();
            for index in 0..u.child_batch.len() {
                let mut row = u.child_batch.row(index)?;
                if index == 0 {
                    row.signature[0] ^= 1;
                }
                rows.push(row.encode()?);
            }
            let changed = Arc::new(AssetSpendBatchV3::from_raw(
                &rows,
                AssetSpendBatchLimitsV3 {
                    transactions: uca_codec().transactions,
                    transaction_bytes: uca_codec().transaction_bytes,
                    body_bytes: uca_codec().body_bytes,
                },
            )?);
            let changed_reference = transport::uca_body_reference(&context, &changed, uca_codec())?;
            let Message::AssetBodyReference {
                transactions: original_ids,
                ..
            } = &reference
            else {
                anyhow::bail!("expected asset reference");
            };
            let Message::AssetBodyReference {
                transactions: changed_ids,
                body_id: changed_id,
                ..
            } = changed_reference
            else {
                anyhow::bail!("expected changed asset reference");
            };
            ensure!(original_ids == &changed_ids && child_id != changed_id);
            Some(changed)
        }
    };
    if matches!(input, ReferenceInput::Exact) {
        // A compact wire recipe does not replace the original retained-body
        // charge. This budget fits reference-sized wire work, not full input.
        let reserve = uca_codec().message_bytes * 2;
        let reference_sized = reserve
            + reference_bytes * 2
            + u.child_batch.canonical_bytes()
            + u.child_batch.len() * 64;
        let mut bounded = CandidateBodies::new(
            &f.base.services.0[first],
            uca_context(f.base.anchor, 1)?,
            policy(),
            uca_codec(),
            BodyLimits {
                bodies: 8,
                bytes: reference_sized,
                inflight: 2,
            },
        )?;
        ensure!(!bounded.offer_remote_successor(
            &child,
            f.base.ids[next],
            &f.base.services.0[first],
            &f.nodes[first].progress,
            &f.base.set,
        )?);
        ensure!(
            bounded.retained() == (0, 0),
            "reference reduced body accounting"
        );
        let mut full_budget = CandidateBodies::new(
            &f.base.services.0[first],
            uca_context(f.base.anchor, 1)?,
            policy(),
            uca_codec(),
            BodyLimits {
                bodies: 8,
                bytes: reference_sized + 2 * (full_bytes - reference_bytes),
                inflight: 2,
            },
        )?;
        ensure!(full_budget.offer_remote_successor(
            &child,
            f.base.ids[next],
            &f.base.services.0[first],
            &f.nodes[first].progress,
            &f.base.set,
        )?);
        ensure!(full_budget.retained().0 == 1);
    }
    ensure!(f.nodes[next].offer_successor(&child, &selection, &f.base.services.0[next])?);

    let mut resolutions = [0u64; 4];
    let deadline = Instant::now() + DEADLINE;
    while !f
        .nodes
        .iter()
        .enumerate()
        .all(|(i, node)| i == next || node.remote_successor_waiting_for_test(child_id))
    {
        ensure!(
            Instant::now() < deadline,
            "asset reference handoff or full reply stalled"
        );
        let now = Instant::now();
        let wall = 1000 + f.started.elapsed().as_millis() as u64;
        for (i, node) in f.nodes.iter_mut().enumerate() {
            node.poll_io(wall, now)?;
            if let Some((source, received)) = node.take_asset_body_reference()? {
                ensure!(i != next && source == f.base.ids[next]);
                ensure!(
                    received.as_ref() == &reference,
                    "reference changed ordered input/context"
                );
                node.resolve_asset_body_reference(
                    source,
                    &received,
                    local_input.clone(),
                    &f.base.services.0[i],
                )?;
                resolutions[i] += 1;
            }
            node.poll(&f.base.services.0[i], now)?;
            ensure!(!node.is_frozen(), "reference froze node {i}");
        }
        std::thread::yield_now();
    }
    ensure!(f.nodes[next].successor_body_enqueued == 3);
    ensure!(f.nodes[next].successor_body_bytes == 3 * reference_bytes as u64);
    for (i, node) in f.nodes.iter().enumerate() {
        ensure!(node.progress.head().is_none() && node.progress.context().height == 1);
        ensure!(
            !node.progress.retains_body(child_id),
            "reference granted future authority"
        );
        if i == next {
            continue;
        }
        ensure!(resolutions[i] == 1 && node.body_references_received == 1);
        ensure!(node.executed() == 0 && node.successor_metrics() == [0; 5]);
        match input {
            ReferenceInput::Exact => ensure!(
                node.body_references_reused == 1 && node.body_reference_full_requests == 0,
                "exact local bytes requested a full body"
            ),
            ReferenceInput::Missing | ReferenceInput::DifferentSignature => ensure!(
                node.body_references_reused == 0 && node.body_reference_full_requests == 1,
                "missing or different full bytes did not use original recovery"
            ),
        }
    }
    if !matches!(input, ReferenceInput::Exact) {
        ensure!(
            f.nodes[next].served_bodies >= 3,
            "live successor did not serve full replies"
        );
    }

    // Only ordinary current-parent execution and real journal ACK can promote
    // the reconstructed/recovered child. No fixture proposal or QC is supplied.
    for (i, node) in f.nodes.iter_mut().enumerate() {
        if i != next {
            ensure!(node.offer(&parent)?);
        }
    }
    let head = f.await_height(2)?;
    ensure!(f.nodes.iter().all(|node| {
        node.executed() == 2 && node.successor_metrics()[3] == 1 && node.successor_metrics()[4] == 0
    }));
    // Retire the running owners before opening the original databases. A second
    // reader must not wait while an unpolled lifecycle holds the sole I/O ticket.
    drop(std::mem::take(&mut f.nodes));
    f.base.services.shutdown()?;
    for (i, path) in f.base.paths.iter().enumerate() {
        let pipeline = CandidatePipeline::start(configure(path)?, OpenMode::Existing)?;
        let journal = open_journal(
            ValidatorJournal::open(
                &pipeline,
                f.base.genesis,
                f.base.anchor,
                f.base.set.clone(),
                f.base.keys[i].clone(),
            )?,
            &pipeline,
        )?;
        ensure!(
            journal.head() == Some(head),
            "reference cold journal head differs"
        );
        drop(journal);
        let mut read = ArchiveRead::new(2, head, f.base.genesis, f.base.set.clone())?;
        let deadline = Instant::now() + DEADLINE;
        let archived = loop {
            ensure!(Instant::now() < deadline, "reference archive read stalled");
            if let Some(result) = read.poll(&pipeline)? {
                break result.context("reference child archive missing")?;
            }
            std::thread::yield_now();
        };
        let restored = transport::stored_body(archived.stored(), uca_codec())?;
        ensure!(transport::candidate_body_id(&restored, uca_codec())? == child_id);
        ensure!(
            &restored == child.as_ref(),
            "archive stored a reference or different signed input"
        );
        ensure!(archived.point() == head);
        pipeline.shutdown()?;
    }
    Ok(())
}

#[test]
#[ignore = "real AOEM/native sockets; remote successor source/domain and independently reproduced parent are mandatory"]
fn native_successor_uca_remote_wrong_parent_never_executes_and_real_body_recovers() -> Result<()> {
    let mut u = UcaFixture::new()?;
    let f = &mut u.native;
    let first = f.leader(1)?;
    let next = f.leader(2)?;
    let target = (0..4)
        .find(|i| *i != first && *i != next)
        .context("missing follower")?;
    let parent = Arc::new(Message::AssetBody {
        context: uca_context(f.base.anchor, 1)?,
        batch: u.parent_batch.clone(),
    });
    ensure!(f.nodes[next].offer(&parent)?);
    f.until(&[next], "own parent basis", |nodes| {
        nodes[next].executed() == 1
    })?;
    let selection = f.nodes[next]
        .successor_selection()?
        .context("missing next leader basis")?;
    let context = uca_context(selection.parent, 2)?;
    let child = Arc::new(Message::AssetBody {
        context,
        batch: u.child_batch.clone(),
    });
    let source = f.base.ids[next];
    ensure!(!f.nodes[target].offer_remote_successor_for_test(
        &child,
        f.base.ids[first],
        &f.base.services.0[target]
    )?);
    for field in 0..5 {
        let mut wrong = context;
        match field {
            0 => wrong.chain_id += 1,
            1 => wrong.protocol_commitment[0] ^= 1,
            2 => wrong.business_program[0] ^= 1,
            3 => wrong.height += 1,
            _ => wrong.parent_height += 1,
        }
        let wrong = Arc::new(Message::AssetBody {
            context: wrong,
            batch: u.child_batch.clone(),
        });
        ensure!(
            !f.nodes[target].offer_remote_successor_for_test(
                &wrong,
                source,
                &f.base.services.0[target]
            )?,
            "invalid future scope {field}"
        );
    }
    // Correctly scoped transport is still not proof of a state root. Retain
    // exactly one claim; duplicates share it, conflicting bodies cannot add a
    // slot, and even a real local Ready must match EVERY parent field.
    let mut wrong = context;
    wrong.parent_state_root[0] ^= 1;
    let wrong = Arc::new(Message::AssetBody {
        context: wrong,
        batch: u.child_batch.clone(),
    });
    let wrong_id = transport::candidate_body_id(&wrong, uca_codec())?;
    ensure!(f.nodes[target].offer_remote_successor_for_test(
        &wrong,
        source,
        &f.base.services.0[target]
    )?);
    ensure!(f.nodes[target].offer_remote_successor_for_test(
        &wrong,
        source,
        &f.base.services.0[target]
    )?);
    ensure!(!f.nodes[target].offer_remote_successor_for_test(
        &child,
        source,
        &f.base.services.0[target]
    )?);
    ensure!(f.nodes[target].offer(&parent)?);
    f.until(
        &[target],
        "wrong remote root is not own durable root",
        |nodes| nodes[target].executed() == 1,
    )?;
    for _ in 0..32 {
        if let Some((source, reference)) = f.nodes[target].take_asset_body_reference()? {
            f.nodes[target].resolve_asset_body_reference(
                source,
                &reference,
                None,
                &f.base.services.0[target],
            )?;
        }
        f.nodes[target].poll(&f.base.services.0[target], Instant::now())?;
    }
    ensure!(f.nodes[target].remote_successor_waiting_for_test(wrong_id));
    ensure!(
        f.nodes[target].successor_metrics() == [0; 5] && f.nodes[target].progress.head().is_none()
    );
    ensure!(f.nodes[next].offer_successor(&child, &selection, &f.base.services.0[next])?);
    ensure!(f.nodes[first].offer(&parent)?);
    f.await_height(2)?;
    ensure!(
        f.nodes[target].executed() == 2 && f.nodes[target].successor_metrics() == [0, 0, 0, 0, 1]
    );
    ensure!(!f.nodes[target].progress.retains_body(wrong_id));
    drop(std::mem::take(&mut f.nodes));
    f.base.services.shutdown()?;
    Ok(())
}

fn run_local_ready_successor(different_parent: bool) -> Result<()> {
    let mut u = UcaFixture::new()?;
    let f = &mut u.native;
    let first = f.leader(1)?;
    let next = f.leader(2)?;
    ensure!(first != next, "fixture needs a follower as next proposer");
    let context = uca_context(f.base.anchor, 1)?;
    let parent = Arc::new(Message::AssetBody {
        context,
        batch: u.parent_batch.clone(),
    });
    let parent_id = transport::candidate_body_id(&parent, uca_codec())?;
    // No input goes to the current proposer. Only the independently admitted
    // follower can execute; no fixture proposal/QC/registration is supplied.
    ensure!(f.nodes[next].offer(&parent)?);
    ensure!(
        f.nodes[next].successor_selection()?.is_none(),
        "unexecuted input supplied a speculative parent"
    );
    f.until(&[next], "local parent ready without proposal", |nodes| {
        nodes[next].executed() == 1
    })?;
    ensure!(
        !f.nodes[next].progress.retains_body(parent_id)
            && f.nodes[next].progress.pending_message().is_none(),
        "local preparation registered or signed without a proposal"
    );
    let selected = f.nodes[next]
        .successor_selection()?
        .context("durable local Ready unnecessarily waited for parent proposal")?;
    let child = Arc::new(Message::AssetBody {
        context: uca_context(selected.parent, 2)?,
        batch: u.child_batch.clone(),
    });
    let child_id = transport::candidate_body_id(&child, uca_codec())?;
    reject_altered_pins(
        &mut f.nodes[next],
        &f.base.services.0[next],
        &child,
        &selected,
    )?;
    ensure!(f.nodes[next].offer_successor(&child, &selected, &f.base.services.0[next])?);
    f.until(
        &[next],
        "private child ready without parent proposal",
        |nodes| nodes[next].successor_metrics()[2] == 1,
    )?;
    ensure!(
        f.nodes[next].successor_metrics() == [1, 1, 1, 0, 0]
            && !f.nodes[next].progress.retains_body(parent_id)
            && !f.nodes[next].progress.retains_body(child_id)
            && f.nodes[next].progress.pending_message().is_none()
            && f.nodes.iter().all(|node| node.progress.head().is_none()),
        "private durable results acquired proposal/vote/head authority"
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
        "private successor published a durable head"
    );
    drop(journal);

    // Only now let the real proposer execute and the four original journals
    // reach quorum. Changing a signed candidate coordinate gives a different
    // valid parent decision, even when its business effects happen to agree.
    let mut actual_context = context;
    actual_context.timestamp_unix_ms += u64::from(different_parent);
    let actual = Arc::new(Message::AssetBody {
        context: actual_context,
        batch: u.parent_batch.clone(),
    });
    ensure!(f.nodes[first].offer(&actual)?);
    if different_parent {
        let head = f.await_height(1)?;
        ensure!(
            head != selected.parent,
            "fixture failed to change decided parent"
        );
        f.until(
            &[0, 1, 2, 3],
            "wrong-parent private child drained",
            |nodes| nodes[next].successor_metrics()[4] == 1,
        )?;
        ensure!(
            f.nodes[next].successor_metrics() == [1, 1, 1, 0, 1]
                && !f.nodes[next].progress.retains_body(child_id)
                && f.nodes
                    .iter()
                    .all(|node| node.progress.head() == Some(head)),
            "unselected private branch was promoted or published"
        );
        ensure!(
            f.nodes[next].successor_selection()?.is_none_or(|current| {
                current.parent != selected.parent
                    && !Arc::ptr_eq(&current.nonce_after, &selected.nonce_after)
            }),
            "different decided parent reused an old nonce projection"
        );
    } else {
        f.await_height(2)?;
        ensure!(
            f.nodes[next].successor_metrics() == [1, 1, 1, 1, 0]
                && f.nodes.iter().all(|node| node.executed() == 2),
            "exact real parent ACK did not reuse one private execution"
        );
    }
    drop(f.nodes.drain(..));
    f.base.services.shutdown()?;
    Ok(())
}

#[test]
#[ignore = "real AOEM and native sockets; reused signature facts recapture full UCA successor state"]
fn native_successor_uca_reused_signatures_keep_budget_nonce_fee_and_recovery() -> Result<()> {
    run_uca_successor(true)
}

#[test]
#[ignore = "real AOEM parent journal ACK; explicit fixture proposal tests request ownership, not finality or TPS"]
fn native_successor_owned_body_does_not_request_second_full_copy_after_ack() -> Result<()> {
    let mut u = UcaFixture::new()?;
    let f = &mut u.native;
    let first = f.leader(1)?;
    let next = f.leader(2)?;
    let parent = Arc::new(Message::AssetBody {
        context: uca_context(f.base.anchor, 1)?,
        batch: u.parent_batch.clone(),
    });
    ensure!(f.nodes[first].offer(&parent)?);
    f.until(&[first, next], "parent execution before ACK", |n| {
        n[next].executed() == 1
    })?;
    let selection = f.nodes[next]
        .successor_selection()?
        .context("parent basis")?;
    let child = Arc::new(Message::AssetBody {
        context: uca_context(selection.parent, 2)?,
        batch: u.child_batch.clone(),
    });
    let id = transport::candidate_body_id(&child, uca_codec())?;
    // Keep the real owner unpolled to reproduce the exact ACK-before-next-body-
    // poll window deterministically. No fabricated capture, execution or ACK.
    let mut held = CandidateBodies::new(
        &f.base.services.0[first],
        uca_context(f.base.anchor, 1)?,
        policy(),
        uca_codec(),
        BodyLimits {
            bodies: 8,
            bytes: 64 * 1024 * 1024,
            inflight: 2,
        },
    )?;
    ensure!(held.offer_remote_successor(
        &child,
        f.base.ids[next],
        &f.base.services.0[first],
        &f.nodes[first].progress,
        &f.base.set
    )?);
    let head = f.await_height(1)?;
    ensure!(head == selection.parent && f.nodes[first].progress.round() == 0);
    let proposal = novovm_consensus::round_bft::test_vectors::sign_proposal(
        f.nodes[first].progress.context(),
        0,
        [71; 32],
        None,
        &f.base.set,
        &f.base.keys[next],
    )?;
    let offer = Message::Proposal {
        proposal,
        valid_quorum: None,
        body_id: id,
    };
    f.nodes[first].progress.receive(f.base.ids[next], &offer)?;
    ensure!(f.nodes[first]
        .progress
        .missing_bodies()
        .any(|(_, missing)| missing == id));
    ensure!(
        held.requests(&f.nodes[first].progress).is_empty(),
        "complete retained successor requested a second full copy after real parent ACK"
    );
    // Missing/different bytes must still request normally; custody of one body
    // does not vouch for another proposal or confer any execution permission.
    let mut other = offer.clone();
    if let Message::Proposal { body_id, .. } = &mut other {
        body_id[0] ^= 1;
    }
    f.nodes[first].progress.receive(f.base.ids[next], &other)?;
    ensure!(held.requests(&f.nodes[first].progress).len() == 1);
    drop(held);
    drop(std::mem::take(&mut f.nodes));
    f.base.services.shutdown()?;
    Ok(())
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
        ensure!(
            f.nodes[next].successor_selection()?.is_none(),
            "old-round nonce projection survived its selection pin"
        );
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
            if let Some((source, reference)) = f.nodes[next].take_asset_body_reference()? {
                f.nodes[next].resolve_asset_body_reference(
                    source,
                    &reference,
                    None,
                    &f.base.services.0[next],
                )?;
            }
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
