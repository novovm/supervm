//! Structural binding tests only: no signature authentication, AOEM callback,
//! business validity proof, durable completion or finality is claimed.

use super::*;
use crate::native_pipeline::state::tree::{empty_root, read_state_value, stage_state_update};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

#[derive(Default)]
struct SourceStatus {
    reads: AtomicUsize,
    disabled: AtomicBool,
    dropped: AtomicBool,
    requested: Mutex<Vec<NodeHash>>,
}

struct Source {
    memory: Memory,
    status: Arc<SourceStatus>,
}

impl StateNodeReader for Source {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.status.reads.fetch_add(1, Ordering::SeqCst);
        self.status.requested.lock().unwrap().push(*hash);
        if self.status.disabled.load(Ordering::SeqCst) {
            bail!("test source is disabled");
        }
        self.memory.read_node(hash)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.status.dropped.store(true, Ordering::SeqCst);
    }
}

fn context(root: NodeHash) -> BatchContext {
    BatchContext {
        chain_id: 7,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: [3; 32],
        semantic_version: 1,
        effect_contract: [4; 32],
        parent_block_hash: [5; 32],
        parent_height: 6,
        parent_state_root: root,
        parent_receipt_root: [8; 32],
        parent_state_version: 9,
        receipt_codec: [10; 32],
        height: 7,
        slot: 11,
        timestamp_unix_ms: 1_900_000_000_000,
    }
}

fn budget() -> PlanBudget {
    PlanBudget {
        transactions: 16,
        transaction_bytes: 1024,
        body_bytes: 4096,
        access_keys: 16,
    }
}

fn capture_budget() -> CaptureBudget {
    CaptureBudget {
        keys: 16,
        nodes: 4096,
        bytes: 1024 * 1024,
    }
}

fn access(key: &[u8], may_put: bool, may_delete: bool) -> DeclaredAccess {
    DeclaredAccess {
        key: key.to_vec(),
        may_put,
        may_delete,
    }
}

fn accesses() -> Vec<DeclaredAccess> {
    vec![
        access(b"payer", true, true),
        access(b"recipient", true, true),
    ]
}

fn raw() -> Vec<Vec<u8>> {
    // Deliberately not a signed native wire. Structural admission must never
    // be mistaken for the authentication layer that has not been migrated.
    vec![b"unverified-a".to_vec(), b"unverified-b".to_vec()]
}

fn plan(context: BatchContext) -> BatchPlan {
    BatchPlan::new(context, raw(), accesses(), budget()).unwrap()
}

fn put(key: &[u8], value: &[u8]) -> StateChange {
    StateChange::Put {
        key: key.to_vec(),
        value: value.to_vec(),
    }
}

fn delete(key: &[u8]) -> StateChange {
    StateChange::Delete { key: key.to_vec() }
}

fn parent(changes: &[StateChange]) -> (Memory, NodeHash) {
    let update = stage_state_update(&Memory::default(), empty_root(), changes).unwrap();
    (Memory(update.nodes().clone()), update.root())
}

fn error<T>(result: Result<T>) -> String {
    format!("{:#}", result.err().expect("expected rejection"))
}

fn context_digest(context: BatchContext) -> NodeHash {
    let mut digest = Sha256::new();
    context.commit(&mut digest).unwrap();
    digest.finalize().into()
}

#[test]
fn every_context_field_changes_the_canonical_encoding() {
    type Mutation = fn(&mut BatchContext);
    let variants: [(&str, Mutation); 16] = [
        ("chain", |value| value.chain_id += 1),
        ("genesis", |value| value.genesis_config_commitment[0] ^= 1),
        ("protocol", |value| value.protocol_commitment[0] ^= 1),
        ("program", |value| value.business_program[0] ^= 1),
        ("semantic version", |value| value.semantic_version += 1),
        ("effect contract", |value| value.effect_contract[0] ^= 1),
        ("parent block", |value| value.parent_block_hash[0] ^= 1),
        ("parent height", |value| value.parent_height += 1),
        ("parent state", |value| value.parent_state_root[0] ^= 1),
        ("parent receipts", |value| value.parent_receipt_root[0] ^= 1),
        ("parent state version", |value| {
            value.parent_state_version += 1
        }),
        ("receipt codec", |value| value.receipt_codec[0] ^= 1),
        ("height", |value| value.height += 1),
        ("slot", |value| value.slot += 1),
        ("time", |value| value.timestamp_unix_ms += 1),
        ("parent and height", |value| {
            value.parent_height += 1;
            value.height += 1;
        }),
    ];
    let baseline = context([7; 32]);
    let expected = context_digest(baseline);
    let expected_plan = plan(baseline).commitment();
    for (label, mutate) in variants {
        let mut changed = baseline;
        mutate(&mut changed);
        assert_ne!(context_digest(changed), expected, "{label}");
        // The independent height mutations test the codec above and are
        // invalid construction inputs; legal variants must change the plan.
        if changed.validate().is_ok() {
            assert_ne!(plan(changed).commitment(), expected_plan, "{label}");
        } else {
            assert!(BatchPlan::new(changed, raw(), accesses(), budget()).is_err());
        }
    }
}

#[test]
fn raw_order_bytes_count_and_length_boundaries_are_bound() {
    let context = context([7; 32]);
    let make = |transactions| {
        BatchPlan::new(context, transactions, accesses(), budget())
            .unwrap()
            .commitment()
    };
    assert_ne!(make(raw()), make(raw().into_iter().rev().collect()));
    let mut changed = raw();
    changed[0][0] ^= 1;
    assert_ne!(make(raw()), make(changed));
    // Equal concatenation and equal transaction count are not equal input.
    assert_ne!(
        make(vec![b"a".to_vec(), b"bc".to_vec()]),
        make(vec![b"ab".to_vec(), b"c".to_vec()])
    );
    assert_ne!(
        make(vec![b"abc".to_vec()]),
        make(vec![b"a".to_vec(), b"bc".to_vec()])
    );
    let structural = plan(context);
    assert_eq!(structural.canonical_transactions_for_test(), raw());
}

fn signed_structural_rows() -> Vec<Vec<u8>> {
    use crate::native_pipeline::ingress::wire::{
        encode_transfer_v3, signing_message, FeePolicy, TransferV3,
    };
    use ed25519_dalek::{Signer, SigningKey};
    let key = SigningKey::from_bytes(&[42; 32]);
    let public = key.verifying_key().to_bytes();
    [0, 128, u64::MAX]
        .into_iter()
        .map(|nonce| {
            let mut tx = TransferV3 {
                chain_id: 7,
                from: public.to_vec(),
                to: vec![9; 32],
                asset: "NOV".into(),
                amount: u128::MAX - u128::from(nonce),
                nonce,
                fee_policy: FeePolicy {
                    pay_asset: "NOV".into(),
                    max_pay_amount: u128::MAX,
                    slippage_bps: 17,
                },
                signature: Vec::new(),
            };
            let signature = key.sign(&signing_message(&tx).unwrap());
            tx.signature = public.to_vec();
            tx.signature.extend_from_slice(&signature.to_bytes());
            encode_transfer_v3(&tx).unwrap()
        })
        .collect()
}

fn structural_source(
    rows: &[Vec<u8>],
) -> Arc<crate::native_pipeline::ingress::apfl::ApflTransferBatch> {
    use crate::native_pipeline::ingress::apfl::{ApflLimits, ApflTransferBatch};
    Arc::new(
        ApflTransferBatch::from_raw(
            rows,
            ApflLimits {
                transactions: 16,
                transaction_bytes: 1024,
                body_bytes: 4096,
            },
        )
        .unwrap(),
    )
}

#[test]
fn apfl_plan_retains_shared_source_and_exact_raw_commitment() {
    let rows = signed_structural_rows();
    let source = structural_source(&rows);
    let raw = BatchPlan::new(context([7; 32]), rows.clone(), accesses(), budget()).unwrap();
    let plan = BatchPlan::from_source(
        context([7; 32]),
        BatchSource::Apfl(Arc::clone(&source)),
        accesses(),
        budget(),
    )
    .unwrap();
    assert_eq!(Arc::strong_count(&source), 2);
    match &plan.source {
        BatchSource::Apfl(retained) => assert!(Arc::ptr_eq(retained, &source)),
        BatchSource::Raw(_) | BatchSource::Uca(_) => panic!("unexpected structural source"),
    }
    assert!(plan.raw_transactions_if_materialized().is_none());
    assert_eq!(
        raw.raw_transactions_if_materialized(),
        Some(rows.as_slice())
    );
    assert_eq!(plan.commitment(), raw.commitment());
    assert_eq!(plan.transaction_count(), rows.len());
    assert_eq!(
        plan.canonical_body_bytes(),
        rows.iter().map(Vec::len).sum::<usize>()
    );
    assert_eq!(
        plan.max_transaction_bytes(),
        rows.iter().map(Vec::len).max().unwrap()
    );
    for (index, expected) in rows.iter().enumerate() {
        assert!(matches!(
            raw.canonical_transaction(index).unwrap(),
            Cow::Borrowed(_)
        ));
        assert_eq!(
            plan.canonical_transaction(index).unwrap().as_ref(),
            expected
        );
    }
    assert_eq!(plan.canonical_transactions_for_test(), rows);
    assert!(raw.canonical_transaction(3).is_err());
    assert!(plan.canonical_transaction(3).is_err());
    // A codec-boundary projection does not replace or clone the retained owner.
    assert_eq!(Arc::strong_count(&source), 2);
}

#[test]
fn asset_plan_keeps_original_columns_and_exact_raw_commitment_order_and_domain() {
    use novovm_protocol::uca_spend::{
        spend_key_ref_v1, AssetAccountCreationV2, AssetAuthorityUpdateV2, AssetSpendIntentV3,
        SignedAssetSpendV3, MLDSA65_PUBLIC_KEY_BYTES, MLDSA65_SIGNATURE_BYTES,
    };
    use novovm_protocol::uca_spend_batch::{AssetSpendBatchLimitsV3, AssetSpendBatchV3};
    // Structural plan fixture only; real ML-DSA facts are covered by the
    // product compute reuse test, not manufactured by these shaped bytes.
    let public = vec![3; MLDSA65_PUBLIC_KEY_BYTES];
    let rows = [0, 1, 128]
        .into_iter()
        .map(|nonce| {
            let mut signed = SignedAssetSpendV3 {
                intent: AssetSpendIntentV3 {
                    authority: novovm_protocol::uca_spend::AssetSpendAuthorityV3::DelegatedGrant,
                    chain_id: 7,
                    genesis: [1; 32],
                    payer: vec![5; 20],
                    recipient: vec![6; 32],
                    asset: 1,
                    amount: 10,
                    max_fee: 1,
                    nonce,
                    authorization_epoch: 1,
                    spend_key_ref: spend_key_ref_v1(&public).unwrap(),
                    not_before_height: 1,
                    expires_at_height: 50,
                    authority_update: None,
                    recipient_creation: None,
                },
                public_key: public.clone(),
                signature: vec![nonce as u8; MLDSA65_SIGNATURE_BYTES],
            };
            if nonce == 1 {
                signed.intent.amount = 0;
                signed.intent.recipient = signed.intent.payer.clone();
                signed.intent.authority =
                    novovm_protocol::uca_spend::AssetSpendAuthorityV3::Controller;
                signed.intent.authority_update = Some(AssetAuthorityUpdateV2 {
                    use_recovery: false,
                    next_epoch: 2,
                    active: true,
                    management_key_ref: [9; 32],
                    recovery_key_ref: Some([8; 32]),
                    grants: vec![],
                });
            } else if nonce == 128 {
                let creation = AssetAccountCreationV2 {
                    management_key_ref: [11; 32],
                    recovery_key_ref: Some([10; 32]),
                    grants: vec![],
                };
                signed.intent.recipient = creation.account(7, &[1; 32]).unwrap().to_vec();
                signed.intent.recipient_creation = Some(creation);
            }
            signed.encode().unwrap()
        })
        .collect::<Vec<_>>();
    let limits = AssetSpendBatchLimitsV3 {
        transactions: 16,
        transaction_bytes: 8192,
        body_bytes: 32768,
    };
    let budget = PlanBudget {
        transactions: limits.transactions,
        transaction_bytes: limits.transaction_bytes,
        body_bytes: limits.body_bytes,
        access_keys: 16,
    };
    let source = Arc::new(AssetSpendBatchV3::from_raw(&rows, limits).unwrap());
    let build =
        |source, context| BatchPlan::from_source(context, source, accesses(), budget).unwrap();
    let reference = build(BatchSource::Raw(rows.clone()), context([7; 32]));
    let plan = build(BatchSource::Uca(source.clone()), context([7; 32]));
    assert!(matches!(&plan.source, BatchSource::Uca(retained) if Arc::ptr_eq(retained, &source)));
    assert_eq!(Arc::strong_count(&source), 2);
    assert!(plan.raw_transactions_if_materialized().is_none());
    assert_eq!(plan.commitment(), reference.commitment());
    assert_eq!(
        plan.canonical_body_bytes(),
        reference.canonical_body_bytes()
    );
    assert_eq!(
        plan.max_transaction_bytes(),
        reference.max_transaction_bytes()
    );
    for (index, bytes) in rows.iter().enumerate() {
        assert_eq!(plan.canonical_transaction(index).unwrap().as_ref(), bytes);
    }
    let mut reordered = rows.clone();
    reordered.swap(0, 1);
    let reordered_source = Arc::new(AssetSpendBatchV3::from_raw(&reordered, limits).unwrap());
    let reordered_plan = build(BatchSource::Uca(reordered_source), context([7; 32]));
    assert_ne!(reordered_plan.commitment(), plan.commitment());
    assert_eq!(
        reordered_plan.commitment(),
        build(BatchSource::Raw(reordered), context([7; 32])).commitment()
    );
    let mut changed_domain = context([7; 32]);
    changed_domain.chain_id += 1;
    assert_ne!(
        build(BatchSource::Uca(source.clone()), changed_domain).commitment(),
        plan.commitment()
    );
    let duplicate =
        Arc::new(AssetSpendBatchV3::from_raw(&[rows[0].clone(), rows[0].clone()], limits).unwrap());
    assert!(BatchPlan::from_source(
        context([7; 32]),
        BatchSource::Uca(duplicate),
        accesses(),
        budget
    )
    .is_err());
}

#[test]
fn apfl_plan_preserves_duplicate_bounds_order_and_signature_binding() {
    let rows = signed_structural_rows();
    let make = |rows: &[Vec<u8>], budget| {
        BatchPlan::from_source(
            context([7; 32]),
            BatchSource::Apfl(structural_source(rows)),
            accesses(),
            budget,
        )
    };
    let baseline = make(&rows, budget()).unwrap().commitment();
    let duplicate = vec![rows[0].clone(), rows[0].clone()];
    assert!(error(make(&duplicate, budget())).contains("duplicate raw"));
    let mut limited = budget();
    limited.transactions = rows.len() - 1;
    assert!(error(make(&rows, limited)).contains("transaction budget"));
    limited = budget();
    limited.transaction_bytes = rows.iter().map(Vec::len).max().unwrap() - 1;
    assert!(error(make(&rows, limited)).contains("transaction size"));
    limited = budget();
    limited.body_bytes = rows.iter().map(Vec::len).sum::<usize>() - 1;
    assert!(error(make(&rows, limited)).contains("body byte budget"));
    let reversed: Vec<_> = rows.iter().rev().cloned().collect();
    assert_ne!(make(&reversed, budget()).unwrap().commitment(), baseline);
    let mut signature_changed = rows.clone();
    *signature_changed[0].last_mut().unwrap() ^= 1;
    // Structural plan binding does not authenticate this altered signature,
    // but MUST commit to it, not only the unsigned canonical transaction id.
    let changed = make(&signature_changed, budget()).unwrap();
    assert_ne!(changed.commitment(), baseline);
    assert_eq!(
        changed.commitment(),
        BatchPlan::new(context([7; 32]), signature_changed, accesses(), budget())
            .unwrap()
            .commitment()
    );
}

#[test]
fn declarations_are_order_canonical_but_keys_and_each_permission_are_bound() {
    let context = context([7; 32]);
    let make = |declarations| {
        BatchPlan::new(context, raw(), declarations, budget())
            .unwrap()
            .commitment()
    };
    let baseline = vec![access(b"a", false, false), access(b"b", true, true)];
    let expected = make(baseline.clone());
    assert_eq!(expected, make(baseline.iter().rev().cloned().collect()));
    let mut put = baseline.clone();
    put[0].may_put = true;
    assert_ne!(expected, make(put));
    let mut delete = baseline.clone();
    delete[0].may_delete = true;
    assert_ne!(expected, make(delete));
    let mut key = baseline.clone();
    key[0].key = b"different".to_vec();
    assert_ne!(expected, make(key));
    assert_ne!(expected, make(vec![baseline[0].clone()]));
    // Declaration framing distinguishes equal concatenated key bytes.
    assert_ne!(
        make(vec![access(b"a", true, false), access(b"bc", true, false)]),
        make(vec![access(b"ab", true, false), access(b"c", true, false)])
    );
}

#[test]
fn duplicate_raw_or_key_declarations_are_rejected_not_merged() {
    let context = context([7; 32]);
    assert!(error(BatchPlan::new(
        context,
        vec![b"same".to_vec(), b"same".to_vec()],
        accesses(),
        budget(),
    ))
    .contains("duplicate raw"));
    for second in [access(b"same", true, false), access(b"same", false, true)] {
        assert!(error(BatchPlan::new(
            context,
            raw(),
            vec![access(b"same", true, false), second],
            budget(),
        ))
        .contains("must be unique"));
    }
}

#[test]
fn admission_limits_are_inclusive_and_do_not_change_identity() {
    let context = context([7; 32]);
    let transactions = vec![b"abc".to_vec(), b"defgh".to_vec()];
    let exact = PlanBudget {
        transactions: 2,
        transaction_bytes: 5,
        body_bytes: 8,
        access_keys: 2,
    };
    let expected = BatchPlan::new(context, transactions.clone(), accesses(), exact)
        .unwrap()
        .commitment();
    assert_eq!(
        expected,
        BatchPlan::new(context, transactions.clone(), accesses(), budget())
            .unwrap()
            .commitment()
    );
    for too_small in [
        PlanBudget {
            transactions: 1,
            ..exact
        },
        PlanBudget {
            transaction_bytes: 4,
            ..exact
        },
        PlanBudget {
            body_bytes: 7,
            ..exact
        },
        PlanBudget {
            access_keys: 1,
            ..exact
        },
    ] {
        assert!(BatchPlan::new(context, transactions.clone(), accesses(), too_small).is_err());
    }
}

#[test]
fn empty_inputs_and_invalid_keys_fail_but_maximum_key_is_accepted() {
    let context = context([7; 32]);
    for transactions in [vec![], vec![vec![]]] {
        assert!(BatchPlan::new(context, transactions, accesses(), budget()).is_err());
    }
    for declarations in [
        vec![],
        vec![access(b"", true, true)],
        vec![access(&[1; 257], true, true)],
    ] {
        assert!(BatchPlan::new(context, raw(), declarations, budget()).is_err());
    }
    assert!(BatchPlan::new(
        context,
        raw(),
        vec![access(&[1; 256], true, true)],
        budget()
    )
    .is_ok());
}

#[test]
fn domain_commitments_and_semantic_version_cannot_be_empty() {
    let baseline = context([7; 32]);
    for index in 0..7 {
        let mut changed = baseline;
        let field = match index {
            0 => &mut changed.genesis_config_commitment,
            1 => &mut changed.protocol_commitment,
            2 => &mut changed.business_program,
            3 => &mut changed.effect_contract,
            4 => &mut changed.parent_state_root,
            5 => &mut changed.parent_receipt_root,
            _ => &mut changed.receipt_codec,
        };
        *field = [0; 32];
        assert!(BatchPlan::new(changed, raw(), accesses(), budget()).is_err());
    }
    let mut zero_chain = baseline;
    zero_chain.chain_id = 0;
    assert!(BatchPlan::new(zero_chain, raw(), accesses(), budget()).is_err());
    let mut zero_version = baseline;
    zero_version.semantic_version = 0;
    assert!(BatchPlan::new(zero_version, raw(), accesses(), budget()).is_err());
}

#[test]
fn first_block_parent_convention_and_height_continuity_are_explicit() {
    let mut first = context([7; 32]);
    first.parent_height = 0;
    first.parent_block_hash = [0; 32];
    first.height = 1;
    assert!(BatchPlan::new(first, raw(), accesses(), budget()).is_ok());
    let mut nonzero_first = first;
    nonzero_first.parent_block_hash = [5; 32];
    assert!(BatchPlan::new(nonzero_first, raw(), accesses(), budget()).is_err());
    let mut zero_successor = context([7; 32]);
    zero_successor.parent_block_hash = [0; 32];
    assert!(BatchPlan::new(zero_successor, raw(), accesses(), budget()).is_err());
    for height in [0, 6, 8] {
        let mut wrong = context([7; 32]);
        wrong.height = height;
        assert!(BatchPlan::new(wrong, raw(), accesses(), budget()).is_err());
    }
}

#[test]
fn height_and_state_version_overflow_reject_without_wrapping() {
    let mut highest = context([7; 32]);
    highest.parent_height = u64::MAX - 1;
    highest.height = u64::MAX;
    highest.parent_state_version = u64::MAX - 2;
    assert!(BatchPlan::new(highest, raw(), accesses(), budget()).is_ok());
    let mut height_overflow = highest;
    height_overflow.parent_height = u64::MAX;
    height_overflow.height = 0;
    assert!(
        error(BatchPlan::new(height_overflow, raw(), accesses(), budget()))
            .contains("height exhausted")
    );
    let mut state_overflow = highest;
    state_overflow.parent_state_version += 1;
    assert!(
        error(BatchPlan::new(state_overflow, raw(), accesses(), budget()))
            .contains("state version exhausted")
    );
}

#[test]
fn capture_reads_the_plan_root_not_an_alternative_reader_root() {
    let (memory, root) = parent(&[put(b"payer", b"100"), put(b"recipient", b"7")]);
    let (foreign, foreign_root) = parent(&[put(b"payer", b"900"), put(b"recipient", b"9")]);
    assert_ne!(root, foreign_root);
    let status = Arc::new(SourceStatus::default());
    let source = Source {
        memory,
        status: status.clone(),
    };
    let input = plan(context(root))
        .capture(&source, capture_budget())
        .unwrap();
    assert_eq!(status.requested.lock().unwrap().first(), Some(&root));
    assert_eq!(input.plan().context().parent_state_root, root);
    assert_eq!(input.read(b"payer").unwrap(), Some(b"100".to_vec()));
    assert!(
        error(plan(context(root)).capture(&foreign, capture_budget()))
            .contains("source node missing")
    );

    struct WrongContent(Vec<u8>);
    impl StateNodeReader for WrongContent {
        fn read_node(&self, _hash: &NodeHash) -> Result<Option<Vec<u8>>> {
            Ok(Some(self.0.clone()))
        }
    }
    let wrong = WrongContent(foreign.0.get(&foreign_root).unwrap().clone());
    assert!(plan(context(root))
        .capture(&wrong, capture_budget())
        .is_err());
}

#[test]
fn plan_input_and_effects_are_send_sync_static() {
    fn require_owned<T: Send + Sync + 'static>() {}
    require_owned::<BatchPlan>();
    require_owned::<OwnedBatchInput>();
    require_owned::<UnpublishedBatchEffects>();
}

#[test]
fn dropped_source_is_not_used_by_threaded_plan_bound_effects() {
    let (memory, root) = parent(&[put(b"payer", b"100"), put(b"recipient", b"7")]);
    let context = context(root);
    let expected_plan = plan(context).commitment();
    let changes = [
        put(b"payer", b"90"),
        delete(b"recipient"),
        put(b"recipient", b"17"),
    ];
    let expected = stage_state_update(&memory, root, &changes).unwrap();
    let status = Arc::new(SourceStatus::default());
    let source = Source {
        memory,
        status: status.clone(),
    };
    let input = plan(context).capture(&source, capture_budget()).unwrap();
    let reads_at_capture = status.reads.load(Ordering::SeqCst);
    assert!(reads_at_capture > 0);
    status.disabled.store(true, Ordering::SeqCst);
    drop(source);
    assert!(status.dropped.load(Ordering::SeqCst));
    let effects = std::thread::spawn(move || {
        assert_eq!(input.read(b"payer")?, Some(b"100".to_vec()));
        input.stage(&changes)
    })
    .join()
    .unwrap()
    .unwrap();
    assert_eq!(effects.plan_commitment(), expected_plan);
    assert_eq!(effects.context(), &context);
    assert_eq!(effects.update().parent_root(), root);
    assert_eq!(effects.update().root(), expected.root());
    assert_eq!(effects.update().nodes(), expected.nodes());
    assert_eq!(status.reads.load(Ordering::SeqCst), reads_at_capture);
    let mut different = context;
    different.effect_contract[0] ^= 1;
    assert_ne!(effects.plan_commitment(), plan(different).commitment());
}

#[test]
fn capture_budget_does_not_change_statement_or_effect_identity() {
    let (memory, root) = parent(&[put(b"payer", b"100")]);
    let minimal = CaptureBudget {
        keys: 2,
        nodes: 1,
        bytes: 38,
    };
    let generous = capture_budget();
    let first = plan(context(root)).capture(&memory, minimal).unwrap();
    let second = plan(context(root)).capture(&memory, generous).unwrap();
    let changes = [put(b"payer", b"90"), put(b"recipient", b"10")];
    let first = first.stage(&changes).unwrap();
    let second = second.stage(&changes).unwrap();
    assert_eq!(first.plan_commitment(), second.plan_commitment());
    assert_eq!(first.update().root(), second.update().root());
    assert_eq!(first.update().nodes(), second.update().nodes());
    for limited in [
        CaptureBudget { keys: 1, ..minimal },
        CaptureBudget {
            nodes: 0,
            ..minimal
        },
        CaptureBudget {
            bytes: 37,
            ..minimal
        },
    ] {
        assert!(plan(context(root)).capture(&memory, limited).is_err());
    }
}

#[test]
fn unknown_keys_and_undeclared_effects_are_rejected_even_when_sibling_is_captured() {
    let (memory, root) = parent(&[put(b"payer", b"100"), put(b"recipient", b"7")]);
    let make = || {
        BatchPlan::new(
            context(root),
            raw(),
            vec![access(b"payer", false, true)],
            budget(),
        )
        .unwrap()
        .capture(&memory, capture_budget())
        .unwrap()
    };
    let input = make();
    assert!(input.read(b"recipient").is_err());
    assert!(input.read(b"unknown").is_err());
    assert!(make().stage(&[put(b"payer", b"90")]).is_err());
    assert!(make().stage(&[delete(b"recipient")]).is_err());
    assert!(make().stage(&[put(b"unknown", b"invented")]).is_err());
    let removed = make().stage(&[delete(b"payer")]).unwrap();
    assert_eq!(
        read_state_value(&memory, removed.update().root(), b"recipient").unwrap(),
        Some(b"7".to_vec())
    );
}

#[test]
fn identical_plan_commitment_does_not_certify_the_resulting_effects() {
    let (memory, root) = parent(&[put(b"payer", b"100"), put(b"recipient", b"7")]);
    let context = context(root);
    let first = plan(context)
        .capture(&memory, capture_budget())
        .unwrap()
        .stage(&[put(b"payer", b"90")])
        .unwrap();
    let second = plan(context)
        .capture(&memory, capture_budget())
        .unwrap()
        .stage(&[put(b"payer", b"80")])
        .unwrap();

    // Both patches obey the same declarations. Structural binding alone does
    // not decide which (if either) implements the pinned business program.
    assert_eq!(first.plan_commitment(), second.plan_commitment());
    assert_eq!(first.context(), second.context());
    assert_eq!(first.update().parent_root(), root);
    assert_eq!(second.update().parent_root(), root);
    assert_ne!(first.update().root(), second.update().root());
    assert_ne!(first.update().nodes(), second.update().nodes());
}

#[test]
fn missing_or_corrupt_source_is_not_authenticated_absence() {
    let (memory, root) = parent(&[put(b"payer", b"100")]);
    let input = plan(context(root))
        .capture(&memory, capture_budget())
        .unwrap();
    assert_eq!(input.read(b"recipient").unwrap(), None);
    let mut missing = memory.clone();
    missing.0.remove(&root);
    assert!(plan(context(root))
        .capture(&missing, capture_budget())
        .is_err());
    let mut corrupt = memory;
    corrupt.0.get_mut(&root).unwrap()[0] ^= 1;
    assert!(plan(context(root))
        .capture(&corrupt, capture_budget())
        .is_err());
}
