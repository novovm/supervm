//! NOV compiler, not an interpreter. Every mutable business decision below is
//! represented by typed references in one AOEM composition. Rust loops build
//! a bounded graph; none runs between submissions or repairs returned values.
use super::execution::*;
use super::*;
use crate::native_pipeline::business::direct_nov_fee::{
    ClearingFailure, FeeFailure, FeeFailureCode, FeeJournalEntry, FeeQuotationContext,
    PreparedTransferFee, ThresholdState,
};
use crate::native_pipeline::business::quoted_transfer::{TransferOutcome, TransferSnapshot};
use novovm_exec::resident::{AoemInteger1024V1, AoemIntegerInputV1 as V, AoemIntegerPlanNodeV1};
use std::collections::HashMap;

#[cfg(test)]
#[path = "full_effects_tests.rs"]
mod tests;

fn lit(n: u128) -> V {
    V::Literal(AoemInteger1024V1::from_u128(n))
}
type Quad = [V; 4];
fn zero() -> Quad {
    std::array::from_fn(|_| lit(0))
}

// Candidate-local hash-consing of this compiler's eight PURE programs, not a
// cache of state admission, quotes, effects, transactions or completed batches.
// Exact equality (including all 1024 bits) resolves hash collisions. We never
// iterate this table, so randomized hashing cannot change plan order/encoding.
#[derive(Hash, PartialEq, Eq)]
enum BindingKey {
    Literal([u32; 32]),
    Output(u32, u32),
}
impl From<&V> for BindingKey {
    fn from(value: &V) -> Self {
        match value {
            V::Literal(value) => Self::Literal(value.0),
            V::Output { row, column } => Self::Output(*row, *column),
        }
    }
}
#[derive(Hash, PartialEq, Eq)]
struct ExpressionKey {
    program: usize,
    guard: BindingKey,
    inputs: Vec<BindingKey>,
    outputs: usize,
}

#[derive(Default)]
struct Builder {
    nodes: Vec<AoemIntegerPlanNodeV1>,
    expressions: HashMap<ExpressionKey, u32>,
    #[cfg(test)]
    unshared: bool,
}
impl Builder {
    fn call(&mut self, program: usize, guard: &V, inputs: Vec<V>, outputs: usize) -> Vec<V> {
        let key = ExpressionKey {
            program,
            guard: guard.into(),
            inputs: inputs.iter().map(BindingKey::from).collect(),
            outputs,
        };
        let existing = self.expressions.get(&key).copied();
        #[cfg(test)]
        let existing = if self.unshared { None } else { existing };
        let row = existing.unwrap_or_else(|| {
            let row = self.nodes.len() as u32;
            self.nodes.push(AoemIntegerPlanNodeV1 {
                program,
                guard: *guard,
                inputs,
                // Every call here has the same zero fallback for its arity.
                // If nonzero fallbacks are added, they MUST enter the key too.
                otherwise: vec![lit(0); outputs],
            });
            self.expressions.insert(key, row);
            row
        });
        (0..outputs)
            .map(|column| V::Output {
                row,
                column: column as u32,
            })
            .collect()
    }
    fn op(&mut self, mode: u128, a: &V, b: &V) -> V {
        // Algebraic identities only: no evaluation of a business after-state.
        // The operands denote pure values; checked guards stay in the graph.
        if b == &lit(0) && matches!(mode, 0 | 1) {
            return *a;
        }
        if a == &lit(0) && mode == 0 {
            return *b;
        }
        if mode == 2 {
            if a == &lit(0) || b == &lit(0) {
                return lit(0);
            }
            if a == &lit(1) {
                return *b;
            }
            if b == &lit(1) {
                return *a;
            }
        }
        if a == b {
            match mode {
                1 => return lit(0),
                3 | 4 => return lit(1),
                6 => return *a,
                _ => {}
            }
        }
        self.call(1, &lit(1), vec![*a, *b, lit(0), lit(mode)], 1)
            .remove(0)
    }
    fn pick(&mut self, guard: &V, yes: &V, no: &V) -> V {
        if guard == &lit(1) || yes == no {
            return *yes;
        }
        if guard == &lit(0) {
            return *no;
        }
        self.call(1, &lit(1), vec![*yes, *no, *guard, lit(5)], 1)
            .remove(0)
    }
    fn add(&mut self, a: &V, b: &V) -> V {
        self.op(0, a, b)
    }
    fn sub(&mut self, a: &V, b: &V) -> V {
        self.op(1, a, b)
    }
    fn mul(&mut self, a: &V, b: &V) -> V {
        self.op(2, a, b)
    }
    fn le(&mut self, a: &V, b: &V) -> V {
        self.op(3, a, b)
    }
    fn eq(&mut self, a: &V, b: &V) -> V {
        self.op(4, a, b)
    }
    fn not(&mut self, a: &V) -> V {
        self.sub(&lit(1), a)
    }
    fn or(&mut self, a: &V, b: &V) -> V {
        let sum = self.add(a, b);
        self.op(6, &sum, &lit(1))
    }
    fn sum(&mut self, a: &Quad, b: &Quad) -> Quad {
        if a == &zero() {
            return *b;
        }
        if b == &zero() {
            return *a;
        }
        if a[1..] == zero()[1..] && b[1..] == zero()[1..] {
            return [self.add(&a[0], &b[0]), lit(0), lit(0), lit(0)];
        }
        self.call(6, &lit(1), a.iter().chain(b).cloned().collect(), 4)
            .try_into()
            .unwrap()
    }
    fn reduce(&mut self, leaves: &[Quad]) -> Quad {
        if leaves.len() == 1 {
            return leaves[0];
        }
        let mid = leaves.len() / 2;
        let left = self.reduce(&leaves[..mid]);
        let right = self.reduce(&leaves[mid..]);
        self.sum(&left, &right)
    }
    // Balanced up/down sweep: O(n) operators, O(log n) depth, no global
    // monetary-state object threaded through independent accounts.
    fn prefixes(&mut self, leaves: &[Quad]) -> (Vec<Quad>, Quad) {
        fn up(b: &mut Builder, a: &[Quad], sums: &mut Vec<Quad>, slot: usize) -> Quad {
            let value = if a.len() == 1 {
                a[0]
            } else {
                let mid = a.len() / 2;
                let l = up(b, &a[..mid], sums, slot * 2);
                let r = up(b, &a[mid..], sums, slot * 2 + 1);
                b.sum(&l, &r)
            };
            sums[slot] = value;
            value
        }
        fn down(b: &mut Builder, sums: &[Quad], slot: usize, out: &mut [Quad], prefix: Quad) {
            if out.len() == 1 {
                out[0] = prefix;
                return;
            }
            let mid = out.len() / 2;
            let right = b.sum(&prefix, &sums[slot * 2]);
            down(b, sums, slot * 2, &mut out[..mid], prefix);
            down(b, sums, slot * 2 + 1, &mut out[mid..], right);
        }
        assert!(!leaves.is_empty());
        let mut sums = vec![zero(); leaves.len() * 4];
        let total = up(self, leaves, &mut sums, 1);
        let mut out = vec![zero(); leaves.len()];
        down(self, &sums, 1, &mut out, zero());
        (out, total)
    }
}

struct Row {
    before: [V; 3],
    after: [V; 5],
    fee_code: V,
    accepted: V,
}

fn static_fee_code(quote: &PreparedTransferFee, policy: &DirectNovFeePolicy, now: u128) -> u128 {
    match quote.quotation() {
        Err(_) => 1,
        Ok(_) if policy.settlement_paused => 2,
        Ok(q) if now > q.expires_at_unix_ms => 3,
        Ok(_) => 0,
    }
}

fn category(b: &mut Builder, row: &Row, static_code: u128, independent: bool, wanted: u128) -> V {
    // Domain of the already compiled outcome: immutable failures take priority;
    // a verified capacity bound excludes capacity failure, not funding failure.
    if static_code != 0 {
        return lit(u128::from(static_code == wanted));
    }
    if matches!(wanted, 1..=3) || (wanted == 5 && independent) {
        return lit(0);
    }
    b.eq(&row.fee_code, &lit(wanted))
}

fn branch(
    b: &mut Builder,
    p: &Prepared,
    quotes: &[PreparedTransferFee],
    guard: &V,
    room: &[V],
    fast: bool,
    now: u128,
) -> Vec<Row> {
    let mut balances: BTreeMap<_, _> = p
        .balances
        .iter()
        .map(|(k, v)| (k.clone(), lit(v.unwrap_or(0))))
        .collect();
    let mut nonces: BTreeMap<_, _> = p
        .nonces
        .iter()
        .map(|(k, v)| (*k, lit((*v).into())))
        .collect();
    let mut prefix = [lit(0), lit(0)];
    let mut rows = Vec::with_capacity(p.requests.len());
    for (i, request) in p.requests.iter().enumerate() {
        let payer = balances[&request.payer];
        let recipient = balances[&request.recipient];
        let nonce = nonces[&p.identities[i]];
        let (fee, cap, _) = quotes[i].intent_terms();
        // These are immutable quote/policy/time constants, not parent-dependent
        // business outcomes. Pause precedes expiry and funding, as on CPU.
        let static_code = static_fee_code(&quotes[i], &p.policy, now);
        let pre = b.call(4, guard, vec![payer, lit(fee), lit(static_code)], 1);
        let decision = if fast {
            vec![lit(0), lit(0), pre[0], b.eq(&pre[0], &lit(0))]
        } else {
            b.call(
                3,
                guard,
                vec![
                    prefix[0],
                    prefix[1],
                    lit(fee),
                    if fast { lit(u128::MAX) } else { room[0] },
                    if fast { lit(u64::MAX.into()) } else { room[1] },
                    pre[0],
                ],
                4,
            )
        };
        if !fast {
            prefix = [decision[0], decision[1]];
        }
        let rejected = b.not(&decision[3]);
        let after: [V; 5] = b
            .call(
                0,
                guard,
                vec![
                    payer,
                    recipient,
                    lit(request.amount),
                    lit(fee),
                    lit(cap),
                    lit(p.tx_nonces[i].into()),
                    nonce,
                    lit(1),
                    lit(u128::from(request.payer == request.recipient)),
                    rejected,
                ],
                5,
            )
            .try_into()
            .unwrap();
        balances.insert(request.payer.clone(), after[0]);
        // A proved credit-only recipient is never a payer, cannot overflow even
        // with every credit, and has its actual ordered receipt prefix below.
        if !fast || !p.credit_only.contains(&request.recipient) {
            balances.insert(request.recipient.clone(), after[1]);
        }
        nonces.insert(p.identities[i], after[2]);
        rows.push(Row {
            before: [payer, recipient, nonce],
            after,
            fee_code: decision[2],
            accepted: decision[3],
        });
    }
    rows
}

/// Exports are the *effective* outputs. No prediction/settlement callback is
/// invoked after this returns. Rows: before[3], after[5], fee code, fee accepted,
/// transfer success, journal sequence, threshold, split[3].
const ROW: usize = 16;
struct Compiled {
    nodes: Vec<AoemIntegerPlanNodeV1>,
    exports: Vec<V>,
    quotes: Vec<PreparedTransferFee>,
}

fn compile(p: &Prepared, now: u128) -> Result<Compiled> {
    compile_with_builder(p, now, Builder::default())
}

fn compile_with_builder(p: &Prepared, now: u128, mut b: Builder) -> Result<Compiled> {
    let context = FeeQuotationContext::new(&p.policy, now)?;
    let quotes = p
        .requests
        .iter()
        .map(|r| context.prepare(r))
        .collect::<Result<Vec<_>>>()?;
    let a = &p.fees.accounting;
    let room = b.call(
        2,
        &lit(1),
        vec![
            lit(a.treasury_reserve_nov.unwrap_or(0)),
            lit(a.settled_nov_total),
            lit(a.settled_by_asset_nov.unwrap_or(0)),
            lit(a.reserve_bucket_nov),
            lit(a.fee_bucket_nov),
            lit(a.risk_buffer_nov),
            lit(a.settlements.into()),
            lit(a.journal_next_seq.into()),
        ],
        2,
    );
    let upper: u128 = quotes.iter().map(|q| q.intent_terms().0).sum(); // <= 104 * bounded rows
    let monetary_safe = b.le(&lit(upper), &room[0]);
    let count_safe = b.le(&lit(quotes.len() as u128), &room[1]);
    let fast = b.mul(&monetary_safe, &count_safe);
    // Select a conservative optimization at compile time from immutable parent
    // bounds, NOT from predicted success/after-state. AOEM recomputes and guards
    // the same sufficient condition, exporting it as a mandatory validity bit.
    // This avoids putting an inactive, long sequential branch into the static
    // depth schedule. No result returns to node to choose or retry a branch.
    let independent = [
        a.treasury_reserve_nov.unwrap_or(0),
        a.settled_nov_total,
        a.settled_by_asset_nov.unwrap_or(0),
    ]
    .iter()
    .all(|v| v.checked_add(upper).is_some())
        && a.reserve_bucket_nov
            .checked_add(a.fee_bucket_nov)
            .and_then(|v| v.checked_add(a.risk_buffer_nov))
            .and_then(|v| v.checked_add(upper))
            .is_some()
        && a.settlements.checked_add(quotes.len() as u64).is_some()
        && a.journal_next_seq
            .checked_add(quotes.len() as u64)
            .is_some();
    let valid = if independent { fast } else { lit(1) };
    let mut rows = branch(&mut b, p, &quotes, &valid, &room, independent, now);
    let successes: Vec<_> = rows.iter().map(|r| b.eq(&r.after[4], &lit(0))).collect();
    let mut credits = BTreeMap::<Account, Vec<usize>>::new();
    for (i, r) in p.requests.iter().enumerate() {
        if p.credit_only.contains(&r.recipient) {
            credits.entry(r.recipient.clone()).or_default().push(i);
        }
    }
    for (account, indices) in credits {
        let leaves: Vec<_> = indices
            .iter()
            .map(|&i| {
                [
                    b.mul(&successes[i], &lit(p.requests[i].amount)),
                    lit(0),
                    lit(0),
                    lit(0),
                ]
            })
            .collect();
        let (prefix, _) = b.prefixes(&leaves);
        for (j, &i) in indices.iter().enumerate() {
            let before = b.add(&lit(p.balances[&account].unwrap_or(0)), &prefix[j][0]);
            rows[i].after[1] = b.add(&before, &leaves[j][0]);
            rows[i].before[1] = before;
        }
    }
    let mut quoted_splits = BTreeMap::<u128, Vec<V>>::new();
    let splits: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            // Rounding depends only on immutable quote and bound policy, never
            // on payer/capacity approval. Execute each distinct division ONCE in
            // AOEM, then guard its three deltas with each real settlement bit.
            // This is not rounding an aggregate fee or a cached settlement.
            let amount = quotes[i].intent_terms().0;
            let split = quoted_splits.entry(amount).or_insert_with(|| {
                b.call(
                    5,
                    &lit(1),
                    vec![
                        lit(amount),
                        lit(p.policy.reserve_share_bps.into()),
                        lit(p.policy.fee_share_bps.into()),
                    ],
                    3,
                )
            });
            split
                .iter()
                .map(|v| b.mul(v, &r.accepted))
                .collect::<Vec<_>>()
        })
        .collect();
    let leaves: Vec<Quad> = rows
        .iter()
        .zip(&splits)
        .map(|(r, s)| [r.after[3], s[0], s[1], r.accepted])
        .collect();
    let (prefix, total) = b.prefixes(&leaves);
    let any_quote = lit(u128::from(quotes.iter().any(|q| q.quotation().is_ok())));
    let day = lit((now / 86_400_000) as u64 as u128);
    let same_day = b.eq(&day, &lit(a.daily_window_day.into()));
    let daily = b.pick(&same_day, &lit(a.daily_nov_used), &lit(0));
    // Every journal is after an accepted quote, hence after day refresh.
    let limit = lit(p.policy.clearing_daily_nov_hard_limit);
    let enabled_limit = lit(u128::from(p.policy.clearing_daily_nov_hard_limit > 0));
    let reached = b.le(&limit, &daily);
    let reached = b.mul(&enabled_limit, &reached);
    let usage = b.mul(&daily, &lit(10_000));
    let usage = b.op(6, &usage, &lit(u128::MAX));
    let near_limit = b.mul(
        &limit,
        &lit(p.policy.clearing_constrained_daily_usage_bps.into()),
    );
    let near_limit = b.op(6, &near_limit, &lit(u128::MAX));
    let near = b.le(&near_limit, &usage);
    let near = b.mul(&enabled_limit, &near);
    let disabled = b.or(&reached, &lit(u128::from(!p.policy.clearing_enabled)));
    let daily_gate = b.pick(&disabled, &lit(2), &near);
    let mut exports = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        let threshold = if p.policy.min_reserve_bucket_nov == 0
            && p.policy.min_fee_bucket_nov == 0
            && !p.policy.clearing_require_healthy_risk_buffer
        {
            // All effective bucket values are checked nonnegative u128; with
            // zero minima and no risk gate only the existing daily gate matters.
            daily_gate
        } else {
            let reserve = b.add(&lit(a.reserve_bucket_nov), &prefix[i][1]);
            let fee = b.add(&lit(a.fee_bucket_nov), &prefix[i][2]);
            let risk_delta = b.sub(&prefix[i][0], &prefix[i][1]);
            let risk_delta = b.sub(&risk_delta, &prefix[i][2]);
            let risk = b.add(&lit(a.risk_buffer_nov), &risk_delta);
            b.call(
                7,
                &lit(1),
                vec![
                    reserve,
                    fee,
                    risk,
                    lit(p.policy.min_reserve_bucket_nov),
                    lit(p.policy.min_fee_bucket_nov),
                    lit(p.policy.min_risk_buffer_nov),
                    lit(u128::from(p.policy.clearing_require_healthy_risk_buffer)),
                    daily_gate,
                ],
                1,
            )
            .remove(0)
        };
        let seq = b.add(&lit(a.journal_next_seq.into()), &prefix[i][3]);
        let seq = b.add(&seq, &r.accepted);
        exports.extend(r.before.iter().chain(&r.after).cloned());
        exports.extend([r.fee_code, r.accepted, successes[i], seq, threshold]);
        exports.extend(splits[i].iter().cloned());
    }
    // Final accounting, including option-presence and window effects. Pure
    // output projection must not infer a monetary decision from old balances.
    let count_zero = b.eq(&total[3], &lit(0));
    let settled = b.not(&count_zero);
    exports.push(settled);
    for old in [
        a.treasury_reserve_nov.unwrap_or(0),
        a.settled_nov_total,
        a.settled_by_asset_nov.unwrap_or(0),
    ] {
        exports.push(b.add(&lit(old), &total[0]));
    }
    exports.push(b.add(&lit(a.reserve_bucket_nov), &total[1]));
    exports.push(b.add(&lit(a.fee_bucket_nov), &total[2]));
    let risk = b.sub(&total[0], &total[1]);
    let risk = b.sub(&risk, &total[2]);
    exports.push(b.add(&lit(a.risk_buffer_nov), &risk));
    exports.push(b.add(&lit(a.settlements.into()), &total[3]));
    exports.push(b.add(&lit(a.journal_next_seq.into()), &total[3]));
    exports.push(b.pick(&any_quote, &day, &lit(a.daily_window_day.into())));
    exports.push(b.pick(&any_quote, &daily, &lit(a.daily_nov_used)));
    let d = &p.fees.diagnostics;
    for (code, old) in [
        (1, d.quote_max_pay_exceeded),
        (2, d.settlement_paused),
        (5, d.settlement_amount_overflow),
        (3, d.clearing_quote_expired),
        (4, d.clearing_insufficient_user_balance),
    ] {
        let leaves: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                [
                    category(
                        &mut b,
                        r,
                        static_fee_code(&quotes[i], &p.policy, now),
                        independent,
                        code,
                    ),
                    lit(0),
                    lit(0),
                    lit(0),
                ]
            })
            .collect();
        let sum = b.reduce(&leaves);
        let count = b.add(&lit(old.into()), &sum[0]);
        exports.push(b.op(6, &count, &lit(u64::MAX.into())));
    }
    let fallback = if p.policy.resolution_source.starts_with("default_fallback") {
        quotes.iter().filter(|q| q.quotation().is_ok()).count()
    } else {
        0
    };
    let count = b.add(
        &lit(d.settlement_policy_fallback.into()),
        &lit(fallback as u128),
    );
    exports.push(b.op(6, &count, &lit(u64::MAX.into())));
    // Last dynamic diagnostic is a reduction of explicit effect indices; zero
    // means preserve old, not clear it on later successful fees.
    let mut indices: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let code = static_fee_code(&quotes[i], &p.policy, now);
            let expired = category(&mut b, r, code, independent, 3);
            let funds = category(&mut b, r, code, independent, 4);
            let clearing = b.or(&expired, &funds);
            b.mul(&clearing, &lit(i as u128 + 1))
        })
        .collect();
    while indices.len() > 1 {
        indices = indices
            .chunks(2)
            .map(|pair| {
                if pair.len() == 1 {
                    pair[0]
                } else {
                    let left = b.le(&pair[0], &pair[1]);
                    b.pick(&left, &pair[1], &pair[0])
                }
            })
            .collect();
    }
    exports.push(indices.remove(0));
    exports.push(valid);
    // Immutable diagnostic selections are constant-folded plan effects too.
    exports.push(lit(quotes
        .iter()
        .rposition(|q| q.quotation().is_ok())
        .map_or(0, |i| i + 1) as u128));
    exports.push(lit(if quotes.last().unwrap().quotation().is_err() {
        quotes.len()
    } else {
        0
    } as u128));
    ensure!(
        b.nodes.len() <= 65_536
            && b.nodes.iter().map(|n| n.otherwise.len()).sum::<usize>() <= 131_072,
        "complete NOV effect plan exceeds AOEM graph budget"
    );
    Ok(Compiled {
        nodes: b.nodes,
        exports,
        quotes,
    })
}

fn fee_failure(
    code: u128,
    quote: &PreparedTransferFee,
    request: &TransferFeeRequest,
    payer: u128,
    now: u128,
) -> Result<Option<FeeFailure>> {
    let (kind, detail) = match code {
        0 => return Ok(None),
        1 => {
            return Ok(Some(
                quote
                    .quotation()
                    .as_ref()
                    .err()
                    .context("missing refused quote")?
                    .clone(),
            ))
        }
        2 => (
            FeeFailureCode::SettlementPaused,
            "treasury settlement is paused".to_owned(),
        ),
        3 => {
            let q = quote
                .quotation()
                .as_ref()
                .ok()
                .context("missing expired quote")?;
            (
                FeeFailureCode::QuoteExpired,
                format!(
                    "asset=NOV quote_id={} now={now} expires_at={}",
                    q.quote_id, q.expires_at_unix_ms
                ),
            )
        }
        4 => {
            let amount = quote
                .quotation()
                .as_ref()
                .ok()
                .context("missing funding quote")?
                .nov_amount;
            let account: String = request
                .payer
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            (FeeFailureCode::InsufficientUserBalance,format!("asset=NOV nov_fee_asset_debit_failed: account=0x{account} requested={amount} available={payer}"))
        }
        5 => (
            FeeFailureCode::AmountOverflow,
            "direct NOV fee settlement monetary capacity exceeded".to_owned(),
        ),
        _ => anyhow::bail!("invalid plan fee outcome tag"),
    };
    Ok(Some(FeeFailure {
        code: kind,
        reason: format!("{}: {detail}", kind.as_str()),
    }))
}

pub(super) fn execute(
    input: SignatureCheckedInput,
    prepared: &Prepared,
    resources: &mut ResidentTransferResources,
    now: u128,
) -> Result<ExecutedNovBatch> {
    ensure!(
        !prepared.requests.is_empty() && prepared.requests.len() <= resources.capacity(),
        "NOV effect candidate capacity"
    );
    let plan = compile(prepared, now)?;
    let values = resources.run_effects(&plan.nodes, &plan.exports)?;
    let n = prepared.requests.len();
    ensure!(
        values.len() == n * ROW + 21,
        "NOV complete effect output shape"
    );
    ensure!(
        values[n * ROW + 18] == 1,
        "AOEM rejected candidate parallel-capacity precondition"
    );
    let mut balances = prepared.balances.clone();
    let mut nonces = prepared.nonces.clone();
    let mut receipts = Vec::with_capacity(n);
    let policy = &prepared.policy;
    for (i, v) in values[..n * ROW].chunks_exact(ROW).enumerate() {
        let request = &prepared.requests[i];
        ensure!(
            v[9] <= 1 && v[10] <= 1 && v[12] <= 2,
            "NOV effect tag out of range"
        );
        let failure = fee_failure(v[8], &plan.quotes[i], request, v[0], now)?;
        ensure!((v[9] == 1) == failure.is_none(), "fee effect tag mismatch");
        let (intent, _) = super::execution::intent(prepared, i, &plan.quotes[i])?;
        let snapshot = TransferSnapshot {
            payer_balance: v[0],
            recipient_balance: v[1],
            next_nonce: u64::try_from(v[2])?,
        };
        let outcome = TransferOutcome::from_checked_integer_values(
            &intent,
            &snapshot,
            failure.as_ref().map(|f| f.reason.as_str()),
            v[3..8].try_into().unwrap(),
        )?;
        ensure!(
            (v[10] == 1) == outcome.is_success(),
            "transfer effect tag mismatch"
        );
        let journal = if v[9] == 1 {
            Some(FeeJournalEntry {
                seq: u64::try_from(v[11])?,
                unix_ms: now,
                tx_hash: request.tx_hash,
                payer: request.payer.clone(),
                source_amount: v[6],
                settled_nov: v[6],
                reserve_bucket_delta_nov: v[13],
                fee_bucket_delta_nov: v[14],
                risk_buffer_delta_nov: v[15],
                policy_version: policy.policy_version,
                policy_source: policy.normalized_source(),
                policy_contract_id: policy.contract_id(),
                policy_threshold_state: match v[12] {
                    0 => ThresholdState::Healthy,
                    1 => ThresholdState::Constrained,
                    _ => ThresholdState::Blocked,
                },
                policy_constrained_strategy: policy.clearing_constrained_strategy.clone(),
            })
        } else {
            None
        };
        // Apply explicit presence/effect tags; no arithmetic, fee settlement,
        // repair or dependent computation is performed during projection.
        if v[9] == 1 {
            balances.insert(request.payer.clone(), Some(v[3]));
        }
        if v[10] == 1 && request.payer != request.recipient {
            balances.insert(request.recipient.clone(), Some(v[4]));
        }
        nonces.insert(prepared.identities[i], u64::try_from(v[5])?);
        receipts.push(NovTransferReceipt {
            tx_hash: request.tx_hash,
            signer_identity: prepared.identities[i],
            delta: outcome.delta().clone(),
            failure: outcome.failure().cloned(),
            quote: plan.quotes[i].quotation().as_ref().ok().cloned(),
            fee_failure: failure,
            journal,
            clear_clearing_candidates: v[9] == 1,
        });
    }
    let f = &values[n * ROW..];
    ensure!(f[0] <= 1, "invalid accounting presence effect");
    let mut fees = prepared.fees.clone();
    let a = &mut fees.accounting;
    if f[0] == 1 {
        a.treasury_reserve_nov = Some(f[1]);
        a.settled_by_asset_nov = Some(f[3]);
    }
    a.settled_nov_total = f[2];
    a.reserve_bucket_nov = f[4];
    a.fee_bucket_nov = f[5];
    a.risk_buffer_nov = f[6];
    a.settlements = u64::try_from(f[7])?;
    a.journal_next_seq = u64::try_from(f[8])?;
    a.daily_window_day = u64::try_from(f[9])?;
    a.daily_nov_used = f[10];
    let d = &mut fees.diagnostics;
    d.quote_max_pay_exceeded = u64::try_from(f[11])?;
    d.settlement_paused = u64::try_from(f[12])?;
    d.settlement_amount_overflow = u64::try_from(f[13])?;
    d.clearing_quote_expired = u64::try_from(f[14])?;
    d.clearing_insufficient_user_balance = u64::try_from(f[15])?;
    d.settlement_policy_fallback = u64::try_from(f[16])?;
    if f[19] != 0 {
        let q = plan
            .quotes
            .get(usize::try_from(f[19] - 1)?)
            .and_then(|q| q.quotation().as_ref().ok())
            .context("invalid selected quote effect")?;
        d.last_quote = Some(q.clone());
    }
    d.last_quote_failure = if f[20] == 0 {
        None
    } else {
        Some(
            plan.quotes
                .get(usize::try_from(f[20] - 1)?)
                .and_then(|q| q.quotation().as_ref().err())
                .context("invalid selected quote failure effect")?
                .clone(),
        )
    };
    if f[17] != 0 {
        let i = usize::try_from(f[17] - 1)?;
        let failure = receipts
            .get(i)
            .and_then(|r| r.fee_failure.clone())
            .context("missing selected clearing effect")?;
        ensure!(
            matches!(
                failure.code,
                FeeFailureCode::QuoteExpired | FeeFailureCode::InsufficientUserBalance
            ),
            "wrong clearing effect category"
        );
        d.last_clearing_failure = Some(ClearingFailure {
            failure,
            unix_ms: now,
        });
    }
    let mut result =
        super::execution::stage_results(input, prepared, balances, nonces, fees, receipts, 0)?;
    result.observation.gpu_rows = n;
    // One complete composition delegation, NOT one physical dispatch.
    result.observation.gpu_waves = 1;
    Ok(result)
}
