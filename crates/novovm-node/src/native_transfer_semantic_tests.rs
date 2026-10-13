//! Historical row oracle for the shared production NOV checked integer asset.
//!
//! Sources: bundled aoem.h AOIP0 v5/APFLOU01 v2/APFLCG01/APFLINT4 contracts,
//! and native_transfer_delta::compute_outcome_v1's existing business rules.
//! This test is neither a second executor nor a default transaction route. The asset
//! contains arithmetic/predicates, not Rust callbacks or precomputed deltas.
//! Authentication, fee quotation/global settlement, state roots, persistence and
//! finality are deliberately not signed off by these isolated row comparisons.

use super::*;
use novovm_exec::{
    AoemExecFacade, AoemInteger1024V1, AoemIntegerOutcomeKindV1, AoemIntegerOutcomeRequestV1,
    AoemIntegerOutcomeRowV1, AoemRuntimeConfig,
};

use crate::native_pipeline::business::nov_transfer_batch::checked_integer_asset::{
    transfer_asset, INPUTS, OUTPUTS,
};

#[derive(Clone)]
struct Case {
    name: String,
    intent: TransferIntent,
    snapshot: TransferSnapshot,
    fee_rejection: Option<&'static str>,
}

impl Case {
    fn new(name: &str, [payer, recipient, amount, fee, cap]: [u128; 5], own: bool) -> Self {
        Self {
            name: name.to_string(),
            intent: TransferIntent {
                tx_hash: [19; 32],
                from: Account::from([1; 32]),
                to: Account::from([if own { 1 } else { 2 }; 32]),
                nonce_identity: "authenticated-signer".to_string(),
                nonce: 17,
                amount,
                approved_fee: fee,
                fee_cap: cap,
            },
            snapshot: TransferSnapshot {
                payer_balance: payer,
                recipient_balance: recipient,
                next_nonce: 17,
            },
            fee_rejection: None,
        }
    }

    fn inputs(&self) -> Vec<AoemInteger1024V1> {
        [
            self.snapshot.payer_balance,
            self.snapshot.recipient_balance,
            self.intent.amount,
            self.intent.approved_fee,
            self.intent.fee_cap,
            u128::from(self.intent.nonce),
            u128::from(self.snapshot.next_nonce),
            u128::from(!self.intent.nonce_identity.is_empty()),
            u128::from(self.intent.from == self.intent.to),
            u128::from(self.fee_rejection.is_some()),
        ]
        .into_iter()
        .map(AoemInteger1024V1::from_u128)
        .collect()
    }

    fn oracle(&self) -> Result<TransferExecutionOutcomeV1, TransferError> {
        compute_outcome_v1(&self.intent, &self.snapshot, self.fee_rejection)
    }
}

fn domain_component(error: &TransferError) -> usize {
    match error {
        TransferError::MissingNonceIdentity => 0,
        TransferError::NonceMismatch { .. } => 1,
        TransferError::NonceExhausted => 2,
        TransferError::InconsistentSelfBalance => 3,
        unexpected => panic!("not a snapshot-domain error: {unexpected:?}"),
    }
}

fn outcome_code(outcome: &TransferExecutionOutcomeV1) -> u128 {
    match outcome.failure() {
        None => 0,
        Some(TransferExecutionFailureV1::Fee(reason))
            if reason.starts_with("fee.quote.max_pay_exceeded:") =>
        {
            2
        }
        Some(TransferExecutionFailureV1::Fee(reason))
            if reason.starts_with("fee.clearing.insufficient_user_balance:") =>
        {
            3
        }
        Some(TransferExecutionFailureV1::Fee(reason)) => {
            assert_eq!(reason, "test.existing_fee_quote_rejection");
            1
        }
        Some(TransferExecutionFailureV1::Business(TransferError::DebitOverflow)) => 4,
        Some(TransferExecutionFailureV1::Business(TransferError::InsufficientFunds { .. })) => 5,
        Some(TransferExecutionFailureV1::Business(TransferError::RecipientOverflow)) => 6,
        other => panic!("unexpected Transfer result: {other:?}"),
    }
}

fn assert_matches(case: &Case, actual: &AoemIntegerOutcomeRowV1) {
    assert_eq!(actual.fault, 0, "{}: {actual:?}", case.name);
    assert_eq!(actual.failed_instruction, None, "{}", case.name);
    match case.oracle() {
        Err(error) => {
            assert_eq!(
                actual.outcome,
                AoemIntegerOutcomeKindV1::OutsideDomain,
                "{}",
                case.name
            );
            assert_eq!(
                actual.component,
                Some(domain_component(&error)),
                "{}",
                case.name
            );
            assert!(
                actual.values.is_empty(),
                "{} must not consume nonce",
                case.name
            );
        }
        Ok(expected) => {
            assert_eq!(
                actual.outcome,
                AoemIntegerOutcomeKindV1::Value,
                "{}: {actual:?}",
                case.name
            );
            assert_eq!(actual.component, None, "{}", case.name);
            let values: Vec<u128> = actual
                .values
                .iter()
                .map(|value| value.try_to_u128().expect("checked full u128 projection"))
                .collect();
            let delta = expected.delta();
            assert_eq!(
                values,
                [
                    delta.payer.after,
                    delta.recipient.after,
                    u128::from(delta.nonce_after),
                    delta.fee_funding_delta,
                    outcome_code(&expected)
                ],
                "{}",
                case.name
            );
        }
    }
}

fn cases() -> Vec<Case> {
    let max = u128::MAX;
    let mut cases = vec![
        Case::new("success", [100, 7, 20, 3, 3], false),
        Case::new("full-u128-success", [max, 0, max - 1, 1, 1], false),
        Case::new("full-u128-fee", [max, 0, 0, max, max], false),
        Case::new(
            "unsigned-high-bit",
            [1 << 127, 0, (1 << 127) - 1, 1, 1],
            false,
        ),
        Case::new("recipient-exact-max", [20, max - 17, 17, 3, 3], false),
        Case::new("zero", [0, 0, 0, 0, 0], false),
        Case::new("fee-cap-before-overflow", [0, max, max, 2, 1], false),
        Case::new("fee-funds-before-overflow", [0, max, max, 1, 1], false),
        Case::new("debit-overflow-before-funds", [max, max, max, 1, 1], false),
        Case::new(
            "business-funds-before-recipient",
            [10, max, 10, 1, 1],
            false,
        ),
        Case::new("recipient-overflow", [100, max, 10, 1, 1], false),
        Case::new("self-max-success", [max, max, max - 1, 1, 1], true),
        Case::new("self-cannot-net-away-debit", [10, 10, 10, 1, 1], true),
        Case::new("self-debit-overflow", [max, max, max, 1, 1], true),
        Case::new("self-full-fee", [max, max, 0, max, max], true),
        Case::new(
            "quote-before-every-business-failure",
            [0, max, max, max, 0],
            false,
        ),
        Case::new("empty-identity-first", [1, 2, max, max, 0], true),
        Case::new("nonce-mismatch-before-exhausted", [1, 2, 0, 0, 0], true),
        Case::new("nonce-exhausted-before-self", [1, 2, 0, 0, 0], true),
        Case::new("self-inconsistent-before-quote", [1, 2, 0, 0, 0], true),
        Case::new("last-valid-nonce", [20, 0, 1, 1, 1], false),
    ];
    cases[15].fee_rejection = Some("test.existing_fee_quote_rejection");
    cases[16].intent.nonce_identity.clear();
    cases[16].intent.nonce = u64::MAX;
    cases[16].fee_rejection = Some("test.existing_fee_quote_rejection");
    cases[17].intent.nonce = u64::MAX;
    cases[18].intent.nonce = u64::MAX;
    cases[18].snapshot.next_nonce = u64::MAX;
    cases[19].fee_rejection = Some("test.existing_fee_quote_rejection");
    cases[20].intent.nonce = u64::MAX - 1;
    cases[20].snapshot.next_nonce = u64::MAX - 1;
    // Deterministic boundary grid, not a synthetic load/TPS test. Many rows use
    // both halves of u128 and scratch sums above u128::MAX.
    let edges = [0, 1, 9, 1 << 64, i128::MAX as u128, 1 << 127, max - 1, max];
    for (i, payer) in edges.into_iter().enumerate() {
        for (j, amount) in edges.into_iter().enumerate() {
            for own in [false, true] {
                let recipient = if own {
                    payer
                } else {
                    edges[(i + j + 3) % edges.len()]
                };
                let fee = edges[(i * 3 + j) % edges.len()];
                let cap = if (i + j) % 3 == 0 { fee / 2 } else { fee };
                cases.push(Case::new(
                    &format!("grid-{i}-{j}-self{own}"),
                    [payer, recipient, amount, fee, cap],
                    own,
                ));
            }
        }
    }
    cases
}

#[test]
fn transfer_semantic_asset_has_bounded_wide_shape_and_real_predicates() {
    let bytes = transfer_asset();
    assert_eq!(&bytes[..8], b"APFLOU01");
    let header: Vec<_> = bytes[8..36]
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    assert_eq!(header, [2, 10, 5, 4, 0, 4, 1024]);
    assert!(INPUTS + OUTPUTS <= 16 && bytes.len() < 4 * 1024 * 1024);
    assert_eq!(
        bytes,
        transfer_asset(),
        "fixed policy compilation must be deterministic"
    );
    let mut offset = 36;
    let mut graphs = 0;
    while offset < bytes.len() {
        let len = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        assert_eq!(&bytes[offset..offset + 8], b"APFLCG01");
        assert!(len > 28);
        offset += len;
        graphs += 1;
    }
    assert_eq!(graphs, 9);
    assert_eq!(offset, bytes.len());
}

#[test]
fn transfer_semantic_rows_preserve_full_width_and_existing_failure_priority() {
    let cases = cases();
    assert!(cases.len() >= 64);
    let expected_codes = [0, 0, 0, 0, 0, 0, 2, 3, 4, 5, 6, 0, 5, 4, 0, 1];
    for (case, expected) in cases.iter().zip(expected_codes) {
        assert_eq!(
            outcome_code(&case.oracle().unwrap()),
            expected,
            "{}",
            case.name
        );
    }
    for (index, component) in [(16, 0), (17, 1), (18, 2), (19, 3)] {
        assert_eq!(
            domain_component(&cases[index].oracle().unwrap_err()),
            component
        );
    }
    for case in &cases {
        let inputs = case.inputs();
        assert_eq!(inputs.len(), INPUTS);
        assert_eq!(
            inputs[0].try_to_u128().unwrap(),
            case.snapshot.payer_balance
        );
        assert_eq!(
            inputs[1].try_to_u128().unwrap(),
            case.snapshot.recipient_balance
        );
        assert_eq!(inputs[2].try_to_u128().unwrap(), case.intent.amount);
        assert_eq!(inputs[3].try_to_u128().unwrap(), case.intent.approved_fee);
    }
    assert_eq!(cases[1].inputs()[0].0[..4], [u32::MAX; 4]);
    assert!(cases[1].inputs()[0].0[4..].iter().all(|limb| *limb == 0));
}

#[test]
#[ignore = "requires packaged AOEM typed i1024 Vulkan runtime; run explicitly for real GPU evidence"]
fn real_aoem_transfer_semantic_outcomes_match_existing_full_u128_rules() {
    let cases = cases();
    let request = AoemIntegerOutcomeRequestV1 {
        program: transfer_asset(),
        input_count: INPUTS,
        output_count: OUTPUTS,
        rows: cases.iter().map(Case::inputs).collect(),
    };
    let runtime = AoemRuntimeConfig::from_env().expect("configured packaged AOEM runtime");
    let facade = AoemExecFacade::open_with_runtime(&runtime).expect("real AOEM facade");
    let session = facade.create_session().expect("one AOEM execution session");
    let first_started = std::time::Instant::now();
    let first = session
        .execute_integer_outcome_v1("nov-test/transfer-semantic", &request)
        .expect("real AOEM typed arithmetic; no Host fallback");
    let first_elapsed = first_started.elapsed();
    assert_eq!(first.rows.len(), cases.len());
    for (case, actual) in cases.iter().zip(&first.rows) {
        assert_matches(case, actual);
    }
    // Same session, same public prefix: facade must consume this invocation's
    // results, not stale output from the previous call.
    let replay_started = std::time::Instant::now();
    let replay = session
        .execute_integer_outcome_v1("nov-test/transfer-semantic", &request)
        .expect("repeat real AOEM typed execution");
    let replay_elapsed = replay_started.elapsed();
    assert_eq!(replay, first);
    eprintln!("Transfer semantic GPU comparison: rows={} same_session_calls=2 first_execute_us={} repeated_execute_us={} GPU_per_request_setup_included=true production_path_changed=false NOT_a_mainchain_TPS_benchmark=true", cases.len(), first_elapsed.as_micros(), replay_elapsed.as_micros());
}
