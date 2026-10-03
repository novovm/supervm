//! Parent-independent preparation is not state admission. The pure tests use
//! real Ed25519 authentication but do not claim native AOEM execution.

use super::*;
use crate::native_pipeline::ingress::batch::{authenticate_batch_for_proof, AuthenticationBudget};
use crate::native_pipeline::ingress::wire::{
    decode_transfer_v3, encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use crate::native_pipeline::state::tree::{empty_root, stage_state_update};
use ed25519_dalek::{Signer, SigningKey};

const CHAIN: u64 = 717;

#[derive(Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15_000,
        policy_version: 1,
        policy_source: "default".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 7000,
        fee_share_bps: 2000,
        risk_buffer_share_bps: 1000,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1_000_000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn signed_with(seed: u8, nonce: u64, edit: impl FnOnce(&mut TransferV3)) -> Vec<u8> {
    let signer = SigningKey::from_bytes(&[seed; 32]);
    let public = signer.verifying_key().to_bytes();
    let mut tx = TransferV3 {
        chain_id: CHAIN,
        from: Sha256::digest(public)[12..].to_vec(),
        to: vec![90; 20],
        asset: "NOV".into(),
        amount: 10,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 0,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    };
    edit(&mut tx);
    let signature = signer.sign(&signing_message(&tx).unwrap());
    tx.signature = public.to_vec();
    tx.signature.extend_from_slice(&signature.to_bytes());
    encode_transfer_v3(&tx).unwrap()
}

fn signed(seed: u8, nonce: u64) -> Vec<u8> {
    signed_with(seed, nonce, |_| {})
}

fn auth_budget() -> AuthenticationBudget {
    AuthenticationBudget {
        transactions: 16,
        transaction_bytes: 4096,
        body_bytes: 65_536,
    }
}

fn budget() -> PlanBudget {
    PlanBudget {
        transactions: 16,
        transaction_bytes: 4096,
        body_bytes: 65_536,
        access_keys: 4096,
    }
}

fn capture_budget() -> CaptureBudget {
    CaptureBudget {
        keys: 4096,
        nodes: 4096,
        bytes: 1 << 20,
    }
}

fn checked(raw: &[Vec<u8>]) -> SignatureCheckedBatch {
    authenticate_batch_for_proof(CHAIN, raw.to_vec(), auth_budget()).unwrap()
}

fn body(raw: &[Vec<u8>]) -> NovTransferBody {
    NovTransferBody::prepare(checked(raw), policy(), budget()).unwrap()
}

fn context(root: NodeHash) -> BatchContext {
    BatchContext {
        chain_id: CHAIN,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(&policy()).unwrap(),
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: root,
        parent_receipt_root: empty_root(),
        parent_state_version: 17,
        receipt_codec: receipt_codec(),
        height: 1,
        slot: 1,
        timestamp_unix_ms: 172_800_123,
    }
}

fn parent(raw: &[Vec<u8>], balance: u128, nonce_offset: u64) -> (Memory, NodeHash) {
    let mut changes = fee_record_changes(&policy(), &FeeState::default()).unwrap();
    let mut seen = BTreeSet::new();
    for tx in checked(raw).transactions() {
        if seen.insert(tx.nonce_identity()) {
            let payer = Account::try_from(tx.transfer().from.as_slice()).unwrap();
            changes.push(StateChange::Put {
                key: balance_key(&payer),
                value: balance.to_le_bytes().to_vec(),
            });
            changes.push(StateChange::Put {
                key: nonce_key(&tx.nonce_identity()),
                value: (tx.transfer().nonce + nonce_offset).to_le_bytes().to_vec(),
            });
        }
    }
    let update = stage_state_update(&Memory::default(), empty_root(), &changes).unwrap();
    (Memory(update.nodes().clone()), update.root())
}

// Test-only oracle copied from main@0bca25d's compiler before the split. It
// deliberately does not call NovTransferBody or the new compatibility wrapper.
fn original_compile(
    batch: SignatureCheckedBatch,
    context: BatchContext,
    policy: DirectNovFeePolicy,
    budget: PlanBudget,
) -> Result<NovTransferPlan> {
    ensure!(
        context.business_program == program_id(),
        "unsupported NOV business program"
    );
    ensure!(
        context.semantic_version == SEMANTIC_VERSION,
        "unsupported NOV semantic version"
    );
    ensure!(
        context.receipt_codec == receipt_codec(),
        "unsupported NOV receipt codec"
    );
    ensure!(
        context.effect_contract == effect_contract(&policy)?,
        "fee/effect policy commitment mismatch"
    );
    let mut keys = BTreeSet::new();
    let mut requests = Vec::with_capacity(batch.transactions().len());
    for authenticated in batch.transactions() {
        let tx = authenticated.transfer();
        ensure!(
            is_nov(&tx.asset) && is_nov(&tx.fee_policy.pay_asset),
            "NOV direct profile does not support this asset or fee asset"
        );
        let payer = Account::try_from(tx.from.as_slice()).map_err(anyhow::Error::msg)?;
        let recipient = Account::try_from(tx.to.as_slice()).map_err(anyhow::Error::msg)?;
        keys.insert(balance_key(&payer));
        keys.insert(balance_key(&recipient));
        keys.insert(nonce_key(&authenticated.nonce_identity()));
        requests.push(TransferFeeRequest {
            tx_hash: authenticated.tx_hash(),
            payer,
            recipient,
            asset: tx.asset.clone(),
            amount: tx.amount,
            pay_asset: tx.fee_policy.pay_asset.clone(),
            max_pay_amount: tx.fee_policy.max_pay_amount,
            slippage_bps: tx.fee_policy.slippage_bps,
        });
    }
    let mut declarations: Vec<_> = keys
        .into_iter()
        .map(|key| DeclaredAccess {
            key,
            may_put: true,
            may_delete: false,
        })
        .collect();
    declarations.extend(record_pages::declarations(
        POLICY_PREFIX,
        POLICY_BYTES,
        false,
    )?);
    declarations.extend(record_pages::declarations(FEE_PREFIX, FEE_BYTES, true)?);
    ensure!(
        declarations.len() <= 4096,
        "NOV effect key budget exceeds tree bound"
    );
    Ok(NovTransferPlan {
        plan: batch.bind(context, declarations, budget)?,
        policy,
        requests,
    })
}

fn access(plan: &NovTransferPlan) -> Vec<(&[u8], bool, bool)> {
    plan.plan()
        .declared_access()
        .iter()
        .map(|item| (item.key.as_slice(), item.may_put, item.may_delete))
        .collect()
}

fn assert_same_plan(left: &NovTransferPlan, right: &NovTransferPlan) {
    assert_eq!(left.commitment(), right.commitment());
    assert_eq!(left.plan().context(), right.plan().context());
    assert_eq!(
        left.plan().raw_transactions(),
        right.plan().raw_transactions()
    );
    assert_eq!(access(left), access(right));
    assert_eq!(left.requests, right.requests);
    assert_eq!(left.policy, right.policy);
}

fn assert_same_execution(left: &ExecutedNovBatch, right: &ExecutedNovBatch) {
    assert_eq!(
        left.effects().plan_commitment(),
        right.effects().plan_commitment()
    );
    assert_eq!(
        left.effects().update().root(),
        right.effects().update().root()
    );
    assert_eq!(
        left.effects().update().nodes(),
        right.effects().update().nodes()
    );
    assert_eq!(left.receipts(), right.receipts());
    assert_eq!(left.fees(), right.fees());
    assert_eq!(
        left.receipt_batch_commitment(),
        right.receipt_batch_commitment()
    );
    assert_eq!(left.statement_commitment(), right.statement_commitment());
}

#[test]
fn body_bind_and_compatibility_compile_match_original_plan_and_complete_effects() {
    let raw = vec![signed(21, 3), signed(22, 5), signed(21, 4)];
    for balance in [0, 10_000] {
        let (memory, root) = parent(&raw, balance, 0);
        let context = context(root);
        let original = original_compile(checked(&raw), context, policy(), budget()).unwrap();
        let split = body(&raw).bind(context).unwrap();
        let compatibility =
            NovTransferPlan::compile(checked(&raw), context, policy(), budget()).unwrap();
        assert_same_plan(&original, &split);
        assert_same_plan(&original, &compatibility);
        let original = original
            .capture(&memory, capture_budget())
            .unwrap()
            .execute_for_proof()
            .unwrap();
        let split = split
            .capture(&memory, capture_budget())
            .unwrap()
            .execute_for_proof()
            .unwrap();
        let compatibility = compatibility
            .capture(&memory, capture_budget())
            .unwrap()
            .execute_for_proof()
            .unwrap();
        assert_same_execution(&original, &split);
        assert_same_execution(&original, &compatibility);
        assert_eq!(split.receipts()[0].delta.nonce_after, 4);
        assert_eq!(split.receipts()[2].delta.nonce_after, 5);
    }
}

#[test]
fn bind_preserves_all_original_domain_shape_rejections() {
    let raw = vec![signed(21, 0)];
    let base = context(empty_root());
    let mutations: &[fn(&mut BatchContext)] = &[
        |c| c.chain_id = 0,
        |c| c.chain_id += 1,
        |c| c.business_program = [9; 32],
        |c| c.semantic_version += 1,
        |c| c.receipt_codec = [9; 32],
        |c| c.effect_contract = [9; 32],
        |c| c.genesis_config_commitment = [0; 32],
        |c| c.protocol_commitment = [0; 32],
        |c| c.parent_state_root = [0; 32],
        |c| c.parent_receipt_root = [0; 32],
        |c| c.height += 1,
        |c| c.parent_block_hash = [9; 32],
        |c| {
            c.parent_height = 1;
            c.height = 2;
        },
        |c| {
            c.parent_height = u64::MAX;
            c.height = 0;
        },
        |c| c.parent_state_version = u64::MAX,
    ];
    for (index, mutation) in mutations.iter().enumerate() {
        let mut invalid = base;
        mutation(&mut invalid);
        assert!(
            original_compile(checked(&raw), invalid, policy(), budget()).is_err(),
            "original {index}"
        );
        assert!(body(&raw).bind(invalid).is_err(), "split {index}");
        assert!(
            NovTransferPlan::compile(checked(&raw), invalid, policy(), budget()).is_err(),
            "compatibility {index}"
        );
    }
}

#[test]
fn valid_context_fields_are_late_bound_but_all_change_the_commitment() {
    let raw = vec![signed(21, 0)];
    let base = context(empty_root());
    let original = body(&raw).bind(base).unwrap().commitment();
    let mutations: &[fn(&mut BatchContext)] = &[
        |c| c.genesis_config_commitment = [7; 32],
        |c| c.protocol_commitment = [7; 32],
        |c| {
            c.parent_block_hash = [7; 32];
            c.parent_height = 1;
            c.height = 2;
        },
        |c| c.parent_state_root = [7; 32],
        |c| c.parent_receipt_root = [7; 32],
        |c| c.parent_state_version += 1,
        |c| c.slot += 1,
        |c| c.timestamp_unix_ms += 1,
    ];
    for mutation in mutations {
        let independent = body(&raw); // Prepared before the parent fields exist.
        let mut changed = base;
        mutation(&mut changed);
        let plan = independent.bind(changed).unwrap();
        assert_ne!(original, plan.commitment());
        assert_same_plan(
            &plan,
            &original_compile(checked(&raw), changed, policy(), budget()).unwrap(),
        );
    }
    // A nonzero claimed genesis/protocol/parent is NOT independently trusted by
    // either compiler. The configured domain and exact parent owner still gate it.
}

#[test]
fn plan_budget_boundaries_match_canonical_constructor_without_relaxation() {
    let raw = vec![signed(21, 0), signed(22, 0)];
    let context = context(empty_root());
    let count = original_compile(checked(&raw), context, policy(), budget())
        .unwrap()
        .plan()
        .declared_access()
        .len();
    let exact = PlanBudget {
        transactions: raw.len(),
        transaction_bytes: raw.iter().map(Vec::len).max().unwrap(),
        body_bytes: raw.iter().map(Vec::len).sum(),
        access_keys: count,
    };
    assert!(NovTransferBody::prepare(checked(&raw), policy(), exact)
        .unwrap()
        .bind(context)
        .is_ok());
    for tight in [
        PlanBudget {
            transactions: exact.transactions - 1,
            ..exact
        },
        PlanBudget {
            transaction_bytes: exact.transaction_bytes - 1,
            ..exact
        },
        PlanBudget {
            body_bytes: exact.body_bytes - 1,
            ..exact
        },
        PlanBudget {
            access_keys: exact.access_keys - 1,
            ..exact
        },
    ] {
        assert!(original_compile(checked(&raw), context, policy(), tight).is_err());
        assert!(NovTransferBody::prepare(checked(&raw), policy(), tight).is_err());
        assert!(NovTransferPlan::compile(checked(&raw), context, policy(), tight).is_err());
    }
}

#[test]
fn signed_fields_cannot_be_replaced_before_independent_preparation() {
    let valid = signed(21, 0);
    let tx = decode_transfer_v3(&valid, 4096).unwrap();
    let mutations: &[fn(&mut TransferV3)] = &[
        |tx| tx.chain_id += 1,
        |tx| tx.from[0] ^= 1,
        |tx| tx.to[0] ^= 1,
        |tx| tx.amount += 1,
        |tx| tx.nonce += 1,
        |tx| tx.asset = "ETH".into(),
        |tx| tx.fee_policy.pay_asset = "ETH".into(),
        |tx| tx.fee_policy.max_pay_amount += 1,
        |tx| tx.fee_policy.slippage_bps += 1,
        |tx| tx.signature[40] ^= 1,
    ];
    for mutate in mutations {
        let mut invalid = tx.clone();
        mutate(&mut invalid);
        assert!(authenticate_batch_for_proof(
            CHAIN,
            vec![encode_transfer_v3(&invalid).unwrap()],
            auth_budget()
        )
        .is_err());
    }
    assert!(
        authenticate_batch_for_proof(CHAIN, vec![valid.clone(), valid], auth_budget()).is_err()
    );
    assert!(authenticate_batch_for_proof(CHAIN, Vec::new(), auth_budget()).is_err());
    let raw = vec![signed(21, 0), signed(22, 0)];
    for tight in [
        AuthenticationBudget {
            transactions: 1,
            ..auth_budget()
        },
        AuthenticationBudget {
            transaction_bytes: raw[0].len() - 1,
            ..auth_budget()
        },
        AuthenticationBudget {
            body_bytes: raw.iter().map(Vec::len).sum::<usize>() - 1,
            ..auth_budget()
        },
    ] {
        assert!(authenticate_batch_for_proof(CHAIN, raw.clone(), tight).is_err());
    }
}

#[test]
fn unsupported_signed_assets_and_wrong_local_policies_do_not_prepare_or_bind() {
    for raw in [
        signed_with(21, 0, |tx| tx.asset = "ETH".into()),
        signed_with(21, 0, |tx| tx.fee_policy.pay_asset = "ETH".into()),
    ] {
        let raw = [raw];
        assert!(
            original_compile(checked(&raw), context(empty_root()), policy(), budget()).is_err()
        );
        assert!(NovTransferBody::prepare(checked(&raw), policy(), budget()).is_err());
    }
    let raw = [signed(21, 0)];
    let mut invalid = policy();
    invalid.fee_share_bps += 1;
    assert!(NovTransferBody::prepare(checked(&raw), invalid, budget()).is_err());
    let mut changed = policy();
    changed.quote_ttl_ms += 1;
    assert!(
        NovTransferBody::prepare(checked(&raw), changed.clone(), budget())
            .unwrap()
            .bind(context(empty_root()))
            .is_err()
    );
    let (memory, root) = parent(&raw, 10_000, 0);
    let mut changed_context = context(root);
    changed_context.effect_contract = effect_contract(&changed).unwrap();
    let plan = NovTransferBody::prepare(checked(&raw), changed, budget())
        .unwrap()
        .bind(changed_context)
        .unwrap();
    assert!(
        plan.capture(&memory, capture_budget()).is_err(),
        "binding cannot replace exact parent policy"
    );
}

#[test]
fn each_parent_requires_new_exact_capture_and_nonce_is_not_preapproved() {
    let raw = [signed(21, 3)];
    let prepared_a = body(&raw);
    let prepared_b = body(&raw);
    let (memory_a, root_a) = parent(&raw, 10_000, 0);
    let (memory_b, root_b) = parent(&raw, 0, 0);
    assert_ne!(root_a, root_b);
    let a = prepared_a
        .bind(context(root_a))
        .unwrap()
        .capture(&memory_a, capture_budget())
        .unwrap();
    let b = prepared_b
        .bind(context(root_b))
        .unwrap()
        .capture(&memory_b, capture_budget())
        .unwrap();
    let payer = a.prepared.requests[0].payer.clone();
    assert_eq!(a.prepared.balances[&payer], Some(10_000));
    assert_eq!(b.prepared.balances[&payer], Some(0));
    assert!(body(&raw)
        .bind(context(root_b))
        .unwrap()
        .capture(&memory_a, capture_budget())
        .is_err());
    let a = a.execute_for_proof().unwrap();
    let b = b.execute_for_proof().unwrap();
    assert!(a.receipts()[0].failure.is_none());
    assert!(b.receipts()[0].failure.is_some());
    assert_eq!(a.receipts()[0].delta.nonce_after, 4);
    assert_eq!(b.receipts()[0].delta.nonce_after, 4);
    assert_ne!(a.effects().update().root(), b.effects().update().root());
    let (replayed, root) = parent(&raw, 10_000, 1);
    let plan = body(&raw).bind(context(root)).unwrap();
    assert!(
        plan.capture(&replayed, capture_budget()).is_err(),
        "only exact capture checks nonce"
    );
}

#[test]
fn compiler_owns_original_policy_and_signed_bytes_and_is_send() {
    fn is_send<T: Send>() {}
    is_send::<NovTransferBody>();
    let mut raw = [signed(21, 0)];
    let original = raw.clone();
    let mut local_policy = policy();
    let independent =
        NovTransferBody::prepare(checked(&raw), local_policy.clone(), budget()).unwrap();
    raw[0][0] ^= 1;
    local_policy.quote_ttl_ms += 1;
    let plan = independent.bind(context(empty_root())).unwrap();
    assert_eq!(plan.plan().raw_transactions(), &original);
    assert_eq!(plan.policy, policy());
    assert_ne!(plan.policy, local_policy);
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; parent-bound economic component, not chain TPS"]
fn native_body_capture_matches_original_compiler_for_each_exact_parent() -> Result<()> {
    use crate::native_pipeline::ingress::batch::authenticate_batch;
    use novovm_exec::resident::ComputeSession;
    use std::time::Duration;
    let library = std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .context("explicit trusted NOVOVM_AOEM_TEST_LIBRARY required")?;
    let mut session = ComputeSession::open(&std::path::PathBuf::from(library), 4)?;
    let raw = vec![signed(21, 3), signed(22, 5), signed(21, 4)];
    for balance in [0, 10_000] {
        let authenticated = authenticate_batch(
            &mut session,
            CHAIN,
            raw.clone(),
            auth_budget(),
            Duration::from_secs(10),
        )?;
        assert!(authenticated.peak_callbacks() > 0);
        let independent = NovTransferBody::prepare(authenticated, policy(), budget())?;
        let (memory, root) = parent(&raw, balance, 0);
        let context = context(root);
        let expected = original_compile(checked(&raw), context, policy(), budget())?
            .capture(&memory, capture_budget())?
            .execute_for_proof()?;
        let actual = independent
            .bind(context)?
            .capture(&memory, capture_budget())?
            .execute(&mut session, Duration::from_secs(10))?;
        assert!(actual.observation().peak_callbacks > 0);
        assert_same_execution(&expected, &actual);
    }
    Ok(())
}
