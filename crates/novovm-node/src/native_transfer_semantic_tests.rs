//! Test-only NOV policy compilation to the existing AOEM typed integer surface.
//!
//! Sources: bundled aoem.h AOIP0 v5/APFLOU01 v2/APFLCG01/APFLINT4 contracts,
//! and native_transfer_delta::compute_outcome_v1's existing business rules.
//! This is neither a second executor nor a default transaction route. The asset
//! contains arithmetic/predicates, not Rust callbacks or precomputed deltas.
//! Authentication, fee quotation/global settlement, state roots, persistence and
//! finality are deliberately not signed off by these isolated row comparisons.

use super::*;
use novovm_exec::{
    AoemExecFacade, AoemInteger1024V1, AoemIntegerOutcomeKindV1, AoemIntegerOutcomeRequestV1,
    AoemIntegerOutcomeRowV1, AoemRuntimeConfig,
};

const INPUTS: usize = 10;
const OUTPUTS: usize = 5;
const CONSTANTS: [i128; 9] = [0, 1, 2, 3, 4, 5, 6, i128::MAX, u64::MAX as i128];

// Inputs: payer, recipient, amount, approved fee, effective cap, supplied nonce,
// expected nonce, nonempty identity, self transfer, existing quote rejection.
// Outputs: payer after, recipient after, nonce after, charged fee, result code.
// Codes: 0 success; 1 quote rejection; 2 cap; 3 fee funds; 4 debit overflow;
// 5 business funds; 6 recipient overflow. Fee and business failures are Values
// with consumed nonce, unlike invalid snapshot inputs (OutsideDomain).

fn words(bytes: &mut Vec<u8>, values: &[u32]) {
    for value in values {
        bytes.extend(value.to_le_bytes());
    }
}

struct Bank {
    inputs: usize,
    steps: Vec<[u32; 4]>,
}

impl Bank {
    fn new(inputs: usize) -> Self {
        assert!(inputs <= 16);
        Self {
            inputs,
            steps: Vec::new(),
        }
    }

    fn constant(&self, index: usize) -> u32 {
        assert!(index < CONSTANTS.len());
        (self.inputs + index) as u32
    }

    fn op(&mut self, opcode: u32, left: u32, right: u32) -> u32 {
        let register = (self.inputs + CONSTANTS.len() + self.steps.len()) as u32;
        assert!(left < register && right < register && opcode <= 11);
        assert!(self.steps.len() < 32, "APFLINT4 Bank instruction limit");
        self.steps.push([opcode, left, right, 0]);
        register
    }

    fn select(&mut self, flag: u32, yes: u32, no: u32) -> u32 {
        // All instructions run. i1024 safely represents negative scratch values
        // and sums beyond u128::MAX; no wrapping/overflow is used as a branch.
        let difference = self.op(1, yes, no);
        let selected = self.op(2, flag, difference);
        self.op(0, no, selected)
    }

    fn max_u128(&mut self) -> u32 {
        // Constants are signed i128 but AOIP0 v5 executes in signed i1024.
        let twice = self.op(0, self.constant(7), self.constant(7));
        self.op(0, twice, self.constant(1))
    }

    fn encode(&self, outputs: &[u32]) -> Vec<u8> {
        assert!((1..=16).contains(&outputs.len()));
        let register_count = self.inputs + CONSTANTS.len() + self.steps.len();
        assert!(outputs.iter().all(|r| (*r as usize) < register_count));
        let mut bytes = b"APFLINT4".to_vec();
        words(
            &mut bytes,
            &[
                2,
                self.inputs as u32,
                self.steps.len() as u32,
                outputs.len() as u32,
                CONSTANTS.len() as u32,
            ],
        );
        words(&mut bytes, &(0..self.inputs as u32).collect::<Vec<_>>());
        for value in CONSTANTS {
            bytes.extend(value.to_le_bytes());
        }
        for instruction in &self.steps {
            words(&mut bytes, instruction);
        }
        words(&mut bytes, outputs);
        bytes
    }
}

struct Graph {
    inputs: usize,
    published: usize,
    calls: Vec<(Vec<u8>, Vec<u32>)>,
}

impl Graph {
    fn new(inputs: usize) -> Self {
        Self {
            inputs,
            published: 0,
            calls: Vec::new(),
        }
    }

    fn call(&mut self, bank: Bank, arguments: Vec<u32>, outputs: &[u32]) -> Vec<u32> {
        assert_eq!(bank.inputs, arguments.len());
        let start = self.inputs + self.published;
        assert!(arguments.iter().all(|r| (*r as usize) < start));
        self.published += outputs.len();
        assert!(self.published <= 64 && self.calls.len() < 16);
        self.calls.push((bank.encode(outputs), arguments));
        (start as u32..(start + outputs.len()) as u32).collect()
    }

    fn encode(self, outputs: &[u32]) -> Vec<u8> {
        assert!(outputs
            .iter()
            .all(|r| (*r as usize) < self.inputs + self.published));
        let mut bytes = b"APFLCG01".to_vec();
        words(
            &mut bytes,
            &[
                2,
                self.inputs as u32,
                self.calls.len() as u32,
                self.calls.len() as u32,
                outputs.len() as u32,
            ],
        );
        for (bank, _) in &self.calls {
            words(&mut bytes, &[bank.len() as u32]);
            bytes.extend(bank);
        }
        for (index, (_, arguments)) in self.calls.iter().enumerate() {
            words(&mut bytes, &[index as u32, arguments.len() as u32]);
            words(&mut bytes, arguments);
        }
        words(&mut bytes, outputs);
        bytes
    }
}

fn predicate(inputs: usize, build: impl FnOnce(&mut Bank) -> u32) -> Vec<u8> {
    let mut bank = Bank::new(inputs);
    let output = build(&mut bank);
    let mut graph = Graph::new(inputs);
    let result = graph.call(bank, (0..inputs as u32).collect(), &[output]);
    graph.encode(&result)
}

fn computation() -> Vec<u8> {
    let mut graph = Graph::new(INPUTS);
    let mut raw = Bank::new(INPUTS);
    let max = raw.max_u128();
    let debit = raw.op(0, 2, 3);
    let credited = raw.op(0, 1, 2);
    let cap_ok = raw.op(10, 3, 4);
    let fee_funded = raw.op(10, 3, 0);
    let debit_fits = raw.op(10, debit, max);
    let debit_funded = raw.op(10, debit, 0);
    let recipient_fits = raw.op(10, credited, max);
    // A self-transfer must afford amount+fee but does not credit amount twice.
    let recipient_ok = raw.select(8, raw.constant(1), recipient_fits);
    let flags = graph.call(
        raw,
        (0..INPUTS as u32).collect(),
        &[cap_ok, fee_funded, debit_fits, debit_funded, recipient_ok],
    );

    let mut status = Bank::new(6);
    let recipient_code = status.select(4, status.constant(0), status.constant(6));
    let funds_code = status.select(3, recipient_code, status.constant(5));
    let business_code = status.select(2, funds_code, status.constant(4));
    let fee_funds_code = status.select(1, status.constant(0), status.constant(3));
    let cap_code = status.select(0, fee_funds_code, status.constant(2));
    let fee_code = status.select(5, status.constant(1), cap_code);
    let fee_allowed = status.op(8, fee_code, status.constant(0));
    let business_ok = status.op(8, business_code, status.constant(0));
    let code = status.select(fee_allowed, business_code, fee_code);
    let mut arguments = flags;
    arguments.push(9);
    let result = graph.call(status, arguments, &[code, fee_allowed, business_ok]);

    let mut effects = Bank::new(9);
    let success = effects.op(2, 7, 8);
    let fee = effects.op(2, 3, 7);
    let amount = effects.op(2, 2, success);
    let other = effects.op(1, effects.constant(1), 5);
    let external_amount = effects.op(2, amount, other);
    let after_fee = effects.op(1, 0, fee);
    let payer = effects.op(1, after_fee, external_amount);
    let credited = effects.op(0, 1, external_amount);
    let recipient = effects.select(5, payer, credited);
    let nonce = effects.op(0, 4, effects.constant(1));
    let outputs = graph.call(
        effects,
        vec![0, 1, 2, 3, 5, 8, result[0], result[1], result[2]],
        &[payer, recipient, nonce, fee, 6],
    );
    graph.encode(&outputs)
}

fn transfer_asset() -> Vec<u8> {
    // Exact validate_snapshot order. Flags derive only from authenticated input
    // identity/equality, not from Host balance or failure computations.
    let domains = vec![
        predicate(INPUTS, |b| b.op(8, 7, b.constant(1))),
        predicate(INPUTS, |b| b.op(8, 5, 6)),
        predicate(INPUTS, |b| b.op(9, 5, b.constant(8))),
        predicate(INPUTS, |b| {
            let equal = b.op(8, 0, 1);
            b.select(8, equal, b.constant(1))
        }),
    ];
    let postconditions = vec![
        predicate(INPUTS + OUTPUTS, |b| {
            let max = b.max_u128();
            let mut valid = b.constant(1);
            for output in [10, 11, 13] {
                let nonnegative = b.op(10, b.constant(0), output);
                let bounded = b.op(10, output, max);
                let both = b.op(2, nonnegative, bounded);
                valid = b.op(2, valid, both);
            }
            valid
        }),
        predicate(INPUTS + OUTPUTS, |b| {
            let expected = b.op(0, 5, b.constant(1));
            let equal = b.op(8, 12, expected);
            let bounded = b.op(10, 12, b.constant(8));
            b.op(2, equal, bounded)
        }),
        predicate(INPUTS + OUTPUTS, |b| {
            let other = b.op(1, b.constant(1), 8);
            let old_recipient = b.op(2, 1, other);
            let old_total = b.op(0, 0, old_recipient);
            let new_recipient = b.op(2, 11, other);
            let balances = b.op(0, 10, new_recipient);
            let new_total = b.op(0, balances, 13);
            let conserved = b.op(8, old_total, new_total);
            let difference = b.op(1, 10, 11);
            let self_difference = b.op(2, 8, difference);
            let consistent = b.op(8, self_difference, b.constant(0));
            b.op(2, conserved, consistent)
        }),
        predicate(INPUTS + OUTPUTS, |b| {
            let lower = b.op(10, b.constant(0), 14);
            let upper = b.op(10, 14, b.constant(6));
            let success = b.op(8, 14, b.constant(0));
            let business_failure = b.op(9, b.constant(3), 14);
            let fee_allowed = b.op(0, success, business_failure);
            let expected_fee = b.op(2, fee_allowed, 3);
            let fee_matches = b.op(8, expected_fee, 13);
            let bounded = b.op(2, lower, upper);
            b.op(2, bounded, fee_matches)
        }),
    ];
    let mut bytes = b"APFLOU01".to_vec();
    words(
        &mut bytes,
        &[
            2,
            INPUTS as u32,
            OUTPUTS as u32,
            domains.len() as u32,
            0,
            postconditions.len() as u32,
            1024,
        ],
    );
    for graph in domains
        .into_iter()
        .chain([computation()])
        .chain(postconditions)
    {
        words(&mut bytes, &[graph.len() as u32]);
        bytes.extend(graph);
    }
    bytes
}

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
