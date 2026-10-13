//! NOV-owned checked transfer algebra for the existing unified AOEM wire.
//! The identical asset is used by product prediction and the historical row oracle.
//! Authentication, quotation and ordered global settlement remain outside this asset.

pub(crate) const INPUTS: usize = 10;
pub(crate) const OUTPUTS: usize = 5;
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

pub(crate) fn transfer_asset() -> Vec<u8> {
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
