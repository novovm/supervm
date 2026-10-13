//! Explicit pure arithmetic for the full candidate compiler; no business callback.
use super::checked_integer_asset::{pure_asset, Bank, Graph};

pub(super) const SHAPES: [(usize, usize); 7] =
    [(4, 1), (8, 2), (6, 4), (3, 1), (3, 3), (8, 4), (8, 1)];

fn single(inputs: usize, make: impl FnOnce(&mut Bank) -> Vec<u32>) -> Vec<u8> {
    let mut bank = Bank::new(inputs);
    let outputs = make(&mut bank);
    let n = outputs.len();
    let mut graph = Graph::new(inputs);
    let result = graph.call(bank, (0..inputs as u32).collect(), &outputs);
    pure_asset(inputs, n, graph.encode(&result))
}

pub(super) fn assets() -> Vec<Vec<u8>> {
    // Scalar instructions: add/sub/mul/le/eq/select/min. All are pure total
    // i1024 expressions for this bounded u128 workload; no effect is performed
    // before a guard selects a value. Division appears only by constant 10000.
    let mut graph = Graph::new(4);
    let mut raw = Bank::new(4);
    let add = raw.op(0, 0, 1);
    let sub = raw.op(1, 0, 1);
    let mul = raw.op(2, 0, 1);
    let le = raw.op(10, 0, 1);
    let eq = raw.op(8, 0, 1);
    let select = raw.select(2, 0, 1);
    let min = raw.select(le, 0, 1);
    let values = graph.call(raw, vec![0, 1, 2, 3], &[add, sub, mul, le, eq, select, min]);
    let mut pick = Bank::new(8);
    let mut result = pick.constant(0);
    for mode in 0..7 {
        let flag = pick.op(8, 7, pick.constant(mode));
        let term = pick.op(2, flag, mode as u32);
        result = pick.op(0, result, term);
    }
    let out = graph.call(pick, [values, vec![3]].concat(), &[result]);
    let scalar = pure_asset(4, 1, graph.encode(&out));

    let capacity = single(8, |b| {
        // Treasury, settled total/asset, three buckets, two checked counters.
        // Bucket sum in i1024 may exceed u128: negative headroom then rejects
        // every positive fee exactly as the old checked-add preflight did.
        let max = b.max_u128();
        let sum = b.op(0, 3, 4);
        let sum = b.op(0, sum, 5);
        let mut room = b.op(1, max, sum);
        for index in 0..3 {
            let next = b.op(1, max, index);
            let smaller = b.op(10, next, room);
            room = b.select(smaller, next, room);
        }
        let count = b.op(10, 6, 7);
        let used = b.select(count, 7, 6);
        let count_room = b.op(1, b.constant(8), used);
        vec![room, count_room]
    });
    let decision = single(6, |b| {
        // Actual charged prefix, count prefix, fee, monetary/count headroom,
        // quote/pause/expiry/funding refusal. Return effective values even on
        // ordinary refusal; it is not a missing operator result.
        let fee_sum = b.op(0, 0, 2);
        let count = b.op(0, 1, b.constant(1));
        let funds_fit = b.op(10, fee_sum, 3);
        let count_fit = b.op(10, count, 4);
        let fits = b.op(2, funds_fit, count_fit);
        let code = b.select(fits, b.constant(0), b.constant(5));
        let pre_ok = b.op(8, 5, b.constant(0));
        let code = b.select(pre_ok, code, 5);
        let accepted = b.op(8, code, b.constant(0));
        let fee_sum = b.select(accepted, fee_sum, 0);
        let count = b.select(accepted, count, 1);
        vec![fee_sum, count, code, accepted]
    });
    let pre = single(3, |b| {
        let funded = b.op(10, 1, 0);
        let code = b.select(funded, b.constant(0), b.constant(4));
        let eligible = b.op(8, 2, b.constant(0));
        let code = b.select(eligible, code, 2);
        vec![code]
    });
    let split = single(3, |b| {
        let reserve = b.op(2, 0, 1);
        let reserve = b.op(7, reserve, b.constant(9));
        let fee = b.op(2, 0, 2);
        let fee = b.op(7, fee, b.constant(9));
        let risk = b.op(1, 0, reserve);
        let risk = b.op(1, risk, fee);
        vec![reserve, fee, risk]
    });
    let sum = single(8, |b| (0..4).map(|i| b.op(0, i, i + 4)).collect());
    let threshold = single(8, |b| {
        let healthy = b.op(10, 5, 2);
        let unhealthy = b.op(1, b.constant(1), healthy);
        let blocked = b.op(2, 6, unhealthy);
        let daily_blocked = b.op(8, 7, b.constant(2));
        let blocked = b.select(daily_blocked, b.constant(1), blocked);
        let reserve_ok = b.op(10, 3, 0);
        let fee_ok = b.op(10, 4, 1);
        let both = b.op(2, reserve_ok, fee_ok);
        let constrained = b.op(1, b.constant(1), both);
        let near = b.op(8, 7, b.constant(1));
        let constrained = b.select(near, b.constant(1), constrained);
        vec![b.select(blocked, b.constant(2), constrained)]
    });
    vec![scalar, capacity, decision, pre, split, sum, threshold]
}
