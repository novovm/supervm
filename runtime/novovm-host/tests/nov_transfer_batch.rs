//! Real AOEM + signed V3 + detached parent execution against an independent
//! serial economic oracle. These are tentative batch effects, not a node,
//! persistent ledger, validity proof, finalized throughput or deployment test.

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use novovm_aoem::ComputeSession;
use novovm_host::business::direct_nov_fee::{
    ClearingFailure, DirectNovFeePolicy, FeeAccounting, FeeFailure, FeeFailureCode,
    FeeJournalEntry, FeeQuote, FeeState, ThresholdState,
};
use novovm_host::business::nov_transfer_batch::{
    balance_key, effect_contract, fee_record_changes, nonce_key, program_id, receipt_codec,
    ExecutedNovBatch, NovTransferPlan, SEMANTIC_VERSION,
};
use novovm_host::business::quoted_transfer::{
    Account, BalanceDelta, TransferDelta, TransferError, TransferFailure,
};
use novovm_host::execution::plan::{BatchContext, PlanBudget};
use novovm_host::ingress::batch::{authenticate_batch, AuthenticationBudget};
use novovm_host::ingress::wire::{
    canonical_tx_hash, encode_transfer_v3, signing_message, FeePolicy, TransferV3,
};
use novovm_host::state::frontier::CaptureBudget;
use novovm_host::state::tree::{
    empty_root, read_state_value, stage_state_update, NodeHash, StateChange, StateNodeReader,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const CHAIN: u64 = 91;
const NOW: u64 = 2 * 86_400_000 + 123;
const TIMEOUT: Duration = Duration::from_secs(30);

fn library() -> PathBuf {
    std::env::var_os("NOVOVM_AOEM_TEST_LIBRARY")
        .expect("explicit trusted NOVOVM_AOEM_TEST_LIBRARY is required")
        .into()
}

fn policy() -> DirectNovFeePolicy {
    DirectNovFeePolicy {
        quote_ttl_ms: 15_000,
        policy_version: 3,
        policy_source: " DEFAULT ".into(),
        resolution_source: "runtime_state".into(),
        reserve_share_bps: 3333,
        fee_share_bps: 3333,
        risk_buffer_share_bps: 3334,
        min_reserve_bucket_nov: 0,
        min_fee_bucket_nov: 0,
        min_risk_buffer_nov: 1,
        settlement_paused: false,
        redeem_paused: false,
        clearing_enabled: true,
        clearing_daily_nov_hard_limit: 1000,
        clearing_require_healthy_risk_buffer: false,
        clearing_constrained_max_slippage_bps: 500,
        clearing_constrained_daily_usage_bps: 8000,
        clearing_constrained_strategy: "daily_volume_only".into(),
        mapped_asset_auto_heal_rollback_enabled: false,
        mapped_asset_reorg_response_policy: "report_only".into(),
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for byte in bytes {
        write!(&mut out, "{byte:02x}").unwrap();
    }
    out
}

fn account(seed: u8, full: bool) -> Account {
    let key = SigningKey::from_bytes(&[seed; 32])
        .verifying_key()
        .to_bytes();
    Account::try_from(if full {
        key.to_vec()
    } else {
        Sha256::digest(key)[12..].to_vec()
    })
    .unwrap()
}

fn transfer(seed: u8, to: Account, amount: u128, nonce: u64) -> TransferV3 {
    TransferV3 {
        chain_id: CHAIN,
        from: account(seed, false).as_bytes().to_vec(),
        to: to.as_bytes().to_vec(),
        asset: "NOV".into(),
        amount,
        nonce,
        fee_policy: FeePolicy {
            pay_asset: "NOV".into(),
            max_pay_amount: 0,
            slippage_bps: 0,
        },
        signature: Vec::new(),
    }
}

struct Signed {
    tx: TransferV3,
    raw: Vec<u8>,
    identity: [u8; 32],
}

fn sign(mut tx: TransferV3, seed: u8) -> Signed {
    let signer = SigningKey::from_bytes(&[seed; 32]);
    let key = signer.verifying_key().to_bytes();
    let signature = signer.sign(&signing_message(&tx).unwrap());
    tx.signature = key.to_vec();
    tx.signature.extend_from_slice(&signature.to_bytes());
    // Independent copy of the existing chain-separated public-key nonce id,
    // not a call into authentication/check_nonce_sequence.
    let mut digest = Sha256::new();
    digest.update(b"novovm-native-auth-nonce-identity-v1");
    digest.update(CHAIN.to_be_bytes());
    digest.update(b"novovm-native-auth/ed25519-public-key/v2\0");
    digest.update(key);
    Signed {
        raw: encode_transfer_v3(&tx).unwrap(),
        tx,
        identity: digest.finalize().into(),
    }
}

#[derive(Clone, Default)]
struct Memory(BTreeMap<NodeHash, Vec<u8>>);

impl StateNodeReader for Memory {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(hash).cloned())
    }
}

struct Source {
    memory: Memory,
    reads: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl StateNodeReader for Source {
    fn read_node(&self, hash: &NodeHash) -> Result<Option<Vec<u8>>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.memory.read_node(hash)
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn budgets() -> (AuthenticationBudget, PlanBudget, CaptureBudget) {
    (
        AuthenticationBudget {
            transactions: 128,
            transaction_bytes: 1024,
            body_bytes: 128 * 1024,
        },
        PlanBudget {
            transactions: 128,
            transaction_bytes: 1024,
            body_bytes: 128 * 1024,
            access_keys: 1024,
        },
        CaptureBudget {
            keys: 1024,
            nodes: 8192,
            bytes: 4 * 1024 * 1024,
        },
    )
}

fn context(root: NodeHash, policy: &DirectNovFeePolicy) -> BatchContext {
    BatchContext {
        chain_id: CHAIN,
        genesis_config_commitment: [1; 32],
        protocol_commitment: [2; 32],
        business_program: program_id(),
        semantic_version: SEMANTIC_VERSION,
        effect_contract: effect_contract(policy).unwrap(),
        parent_block_hash: [0; 32],
        parent_height: 0,
        parent_state_root: root,
        parent_receipt_root: empty_root(),
        parent_state_version: 0,
        receipt_codec: receipt_codec(),
        height: 1,
        slot: 0,
        timestamp_unix_ms: NOW,
    }
}

fn put(key: Vec<u8>, value: Vec<u8>) -> StateChange {
    StateChange::Put { key, value }
}

fn initial_tree(
    policy: &DirectNovFeePolicy,
    fees: &FeeState,
    balances: &BTreeMap<Account, u128>,
    nonces: &BTreeMap<[u8; 32], u64>,
) -> Result<(Memory, NodeHash)> {
    let mut changes = fee_record_changes(policy, fees)?;
    changes.extend(
        balances
            .iter()
            .map(|(key, value)| put(balance_key(key), value.to_le_bytes().to_vec())),
    );
    changes.extend(
        nonces
            .iter()
            .map(|(key, value)| put(nonce_key(key), value.to_le_bytes().to_vec())),
    );
    let update = stage_state_update(&Memory::default(), empty_root(), &changes)?;
    Ok((Memory(update.nodes().clone()), update.root()))
}

// The oracle intentionally does NOT call quote_transfer/quote_and_settle,
// compute_outcome, component execution or the production reducer. It freezes
// the old fee-before-business rule using ordinary checked serial arithmetic.
#[allow(clippy::manual_div_ceil)] // Frozen old arithmetic, independent of the new helper.
fn reference_quote(tx: &TransferV3, now: u128, ttl: u128) -> Result<FeeQuote, FeeFailure> {
    let arguments = format!(
        "{{\"asset\":\"NOV\",\"to\":\"0x{}\",\"amount\":\"{}\"}}",
        hex(&tx.to),
        tx.amount
    );
    let fee = 20 + 8 + 4 + 8 + (((arguments.len() + 15) / 16) as u128).min(64);
    let slippage = tx.fee_policy.slippage_bps.min(10_000);
    let inclusive = (fee * (10_000 + u128::from(slippage)) + 9999) / 10_000;
    let cap = if tx.fee_policy.max_pay_amount == 0 {
        inclusive
    } else {
        tx.fee_policy.max_pay_amount
    };
    if inclusive > cap {
        return Err(FeeFailure {
            code: FeeFailureCode::MaxPayExceeded,
            reason: format!("fee.quote.max_pay_exceeded: required_with_slippage={inclusive} max_pay_amount={cap} pay_asset=NOV"),
        });
    }
    Ok(FeeQuote {
        quote_id: format!("q-{}-{now:x}", hex(&canonical_tx_hash(tx).unwrap()[..6])),
        pay_asset: "NOV".into(),
        nov_amount: fee,
        quoted_pay_amount: fee,
        quoted_pay_amount_with_slippage: inclusive,
        max_pay_amount: cap,
        slippage_bps: slippage,
        quoted_at_unix_ms: now,
        expires_at_unix_ms: now.saturating_add(ttl.max(1)),
        rate_ppm: 1_000_000,
        oracle_updated_at_unix_ms: now,
        route: "direct_nov".into(),
        quote_contract: "novovm-exec-fee-quote/v1".into(),
        price_source: "direct_nov".into(),
    })
}

fn source_name(policy: &DirectNovFeePolicy) -> String {
    let source = policy.policy_source.trim().to_ascii_lowercase();
    if source.is_empty() || source == "default" {
        "config_path".into()
    } else {
        source
    }
}

fn policy_id(p: &DirectNovFeePolicy) -> String {
    format!(
        "nov_treasury_policy_v1:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        p.policy_version,
        source_name(p),
        p.reserve_share_bps,
        p.fee_share_bps,
        p.risk_buffer_share_bps,
        p.min_reserve_bucket_nov,
        p.min_fee_bucket_nov,
        p.min_risk_buffer_nov,
        u8::from(p.settlement_paused),
        u8::from(p.redeem_paused),
        u8::from(p.clearing_enabled),
        p.clearing_daily_nov_hard_limit,
        u8::from(p.clearing_require_healthy_risk_buffer),
        p.clearing_constrained_max_slippage_bps,
        p.clearing_constrained_daily_usage_bps,
        p.clearing_constrained_strategy,
        u8::from(p.mapped_asset_auto_heal_rollback_enabled),
        p.mapped_asset_reorg_response_policy
    )
}

fn reference_threshold(p: &DirectNovFeePolicy, a: &FeeAccounting) -> ThresholdState {
    let limit = p.clearing_daily_nov_hard_limit;
    let reached = limit > 0 && a.daily_nov_used >= limit;
    if !p.clearing_enabled
        || (p.clearing_require_healthy_risk_buffer && a.risk_buffer_nov < p.min_risk_buffer_nov)
        || reached
    {
        ThresholdState::Blocked
    } else if a.reserve_bucket_nov < p.min_reserve_bucket_nov
        || a.fee_bucket_nov < p.min_fee_bucket_nov
        || (limit > 0
            && a.daily_nov_used.saturating_mul(10_000)
                >= limit.saturating_mul(u128::from(p.clearing_constrained_daily_usage_bps)))
    {
        ThresholdState::Constrained
    } else {
        ThresholdState::Healthy
    }
}

fn reference_credit(
    before: &FeeAccounting,
    fee: u128,
    p: &DirectNovFeePolicy,
) -> Option<(FeeAccounting, [u128; 3])> {
    let reserve = fee.checked_mul(u128::from(p.reserve_share_bps))? / 10_000;
    let fee_bucket = fee.checked_mul(u128::from(p.fee_share_bps))? / 10_000;
    let risk = fee.checked_sub(reserve)?.checked_sub(fee_bucket)?;
    let mut after = before.clone();
    after.treasury_reserve_nov = Some(before.treasury_reserve_nov.unwrap_or(0).checked_add(fee)?);
    after.settled_nov_total = before.settled_nov_total.checked_add(fee)?;
    after.settled_by_asset_nov = Some(before.settled_by_asset_nov.unwrap_or(0).checked_add(fee)?);
    after.reserve_bucket_nov = before.reserve_bucket_nov.checked_add(reserve)?;
    after.fee_bucket_nov = before.fee_bucket_nov.checked_add(fee_bucket)?;
    after.risk_buffer_nov = before.risk_buffer_nov.checked_add(risk)?;
    after
        .reserve_bucket_nov
        .checked_add(after.fee_bucket_nov)?
        .checked_add(after.risk_buffer_nov)?;
    after.settlements = before.settlements.checked_add(1)?;
    after.journal_next_seq = before.journal_next_seq.checked_add(1)?;
    Some((after, [reserve, fee_bucket, risk]))
}

struct ExpectedReceipt {
    identity: [u8; 32],
    delta: TransferDelta,
    failure: Option<TransferFailure>,
    quote: Option<FeeQuote>,
    fee_failure: Option<FeeFailure>,
    journal: Option<FeeJournalEntry>,
}

struct Reference {
    balances: BTreeMap<Account, u128>,
    nonces: BTreeMap<[u8; 32], u64>,
    fees: FeeState,
    receipts: Vec<ExpectedReceipt>,
}

fn serial_reference(
    transactions: &[Signed],
    policy: &DirectNovFeePolicy,
    initial_fees: &FeeState,
    balances: &BTreeMap<Account, u128>,
    nonces: &BTreeMap<[u8; 32], u64>,
) -> Reference {
    let mut out = Reference {
        balances: balances.clone(),
        nonces: nonces.clone(),
        fees: initial_fees.clone(),
        receipts: Vec::new(),
    };
    for signed in transactions {
        let tx = &signed.tx;
        let from = Account::try_from(tx.from.as_slice()).unwrap();
        let to = Account::try_from(tx.to.as_slice()).unwrap();
        let payer_before = out.balances.get(&from).copied().unwrap_or(0);
        let recipient_before = out.balances.get(&to).copied().unwrap_or(0);
        assert_eq!(
            out.nonces.get(&signed.identity).copied().unwrap_or(0),
            tx.nonce
        );
        let nonce_after = tx.nonce.checked_add(1).unwrap();
        let hash = canonical_tx_hash(tx).unwrap();
        let now = u128::from(NOW);
        let mut expected = ExpectedReceipt {
            identity: signed.identity,
            delta: TransferDelta {
                tx_hash: hash,
                payer: BalanceDelta {
                    account: from.clone(),
                    before: payer_before,
                    after: payer_before,
                },
                recipient: BalanceDelta {
                    account: to.clone(),
                    before: recipient_before,
                    after: recipient_before,
                },
                nonce_identity: hex(&signed.identity),
                nonce_before: tx.nonce,
                nonce_after,
                fee_funding_delta: 0,
            },
            failure: None,
            quote: None,
            fee_failure: None,
            journal: None,
        };
        match reference_quote(tx, now, policy.quote_ttl_ms) {
            Err(failed) => {
                out.fees.diagnostics.quote_max_pay_exceeded = out
                    .fees
                    .diagnostics
                    .quote_max_pay_exceeded
                    .saturating_add(1);
                out.fees.diagnostics.last_quote_failure = Some(failed.clone());
                expected.fee_failure = Some(failed);
            }
            Ok(quote) => {
                out.fees.diagnostics.last_quote = Some(quote.clone());
                out.fees.diagnostics.last_quote_failure = None;
                let day = (now / 86_400_000) as u64;
                if out.fees.accounting.daily_window_day != day {
                    out.fees.accounting.daily_window_day = day;
                    out.fees.accounting.daily_nov_used = 0;
                }
                let threshold = reference_threshold(policy, &out.fees.accounting);
                if policy.resolution_source.starts_with("default_fallback") {
                    out.fees.diagnostics.settlement_policy_fallback = out
                        .fees
                        .diagnostics
                        .settlement_policy_fallback
                        .saturating_add(1);
                }
                let paid = quote.nov_amount;
                let credited = reference_credit(&out.fees.accounting, paid, policy);
                if policy.settlement_paused {
                    out.fees.diagnostics.settlement_paused =
                        out.fees.diagnostics.settlement_paused.saturating_add(1);
                    expected.fee_failure = Some(FeeFailure {
                        code: FeeFailureCode::SettlementPaused,
                        reason: "fee.settlement.settlement_paused: treasury settlement is paused"
                            .into(),
                    });
                } else if payer_before < paid {
                    out.fees.diagnostics.clearing_insufficient_user_balance = out
                        .fees
                        .diagnostics
                        .clearing_insufficient_user_balance
                        .saturating_add(1);
                    let failure = FeeFailure { code: FeeFailureCode::InsufficientUserBalance, reason: format!("fee.clearing.insufficient_user_balance: asset=NOV nov_fee_asset_debit_failed: account=0x{} requested={paid} available={payer_before}", hex(from.as_bytes())) };
                    out.fees.diagnostics.last_clearing_failure = Some(ClearingFailure {
                        failure: failure.clone(),
                        unix_ms: now,
                    });
                    expected.fee_failure = Some(failure);
                } else if let Some((accounting, [reserve, fee_bucket, risk])) = credited {
                    out.fees.accounting = accounting;
                    out.fees.diagnostics.last_clearing_candidates = [];
                    expected.journal = Some(FeeJournalEntry {
                        seq: out.fees.accounting.journal_next_seq,
                        unix_ms: now,
                        tx_hash: hash,
                        payer: from.clone(),
                        source_amount: paid,
                        settled_nov: paid,
                        reserve_bucket_delta_nov: reserve,
                        fee_bucket_delta_nov: fee_bucket,
                        risk_buffer_delta_nov: risk,
                        policy_version: policy.policy_version,
                        policy_source: source_name(policy),
                        policy_contract_id: policy_id(policy),
                        policy_threshold_state: threshold,
                        policy_constrained_strategy: policy.clearing_constrained_strategy.clone(),
                    });
                    expected.delta.payer.after = payer_before - paid;
                    if from == to {
                        expected.delta.recipient.after = payer_before - paid;
                    }
                    expected.delta.fee_funding_delta = paid;
                    let business = tx
                        .amount
                        .checked_add(paid)
                        .ok_or(TransferError::DebitOverflow)
                        .and_then(|required| {
                            payer_before.checked_sub(required).ok_or(
                                TransferError::InsufficientFunds {
                                    available: payer_before,
                                    required,
                                },
                            )
                        })
                        .and_then(|debited| {
                            if from == to {
                                Ok((
                                    debited.checked_add(tx.amount).unwrap(),
                                    debited.checked_add(tx.amount).unwrap(),
                                ))
                            } else {
                                recipient_before
                                    .checked_add(tx.amount)
                                    .map(|recipient| (debited, recipient))
                                    .ok_or(TransferError::RecipientOverflow)
                            }
                        });
                    match business {
                        Ok((payer, recipient)) => {
                            expected.delta.payer.after = payer;
                            expected.delta.recipient.after = recipient;
                            out.balances.insert(to.clone(), recipient);
                        }
                        Err(failure) => expected.failure = Some(TransferFailure::Business(failure)),
                    }
                    out.balances
                        .insert(from.clone(), expected.delta.payer.after);
                } else {
                    out.fees.diagnostics.settlement_amount_overflow = out
                        .fees
                        .diagnostics
                        .settlement_amount_overflow
                        .saturating_add(1);
                    expected.fee_failure = Some(FeeFailure { code: FeeFailureCode::AmountOverflow, reason: "fee.settlement.amount_overflow: direct NOV fee settlement monetary capacity exceeded".into() });
                }
                expected.quote = Some(quote);
            }
        }
        if let Some(failed) = &expected.fee_failure {
            expected.failure = Some(TransferFailure::Fee(failed.reason.clone()));
        }
        out.nonces.insert(signed.identity, nonce_after);
        out.receipts.push(expected);
    }
    out
}

fn run_and_check(
    session: &mut ComputeSession,
    transactions: &[Signed],
    policy: &DirectNovFeePolicy,
    fees: &FeeState,
    balances: &BTreeMap<Account, u128>,
    nonces: &BTreeMap<[u8; 32], u64>,
) -> Result<ExecutedNovBatch> {
    let reference = serial_reference(transactions, policy, fees, balances, nonces);
    let (memory, root) = initial_tree(policy, fees, balances, nonces)?;
    let initial_nodes = memory.0.clone();
    let (auth_budget, plan_budget, capture_budget) = budgets();
    let authenticated = authenticate_batch(
        session,
        CHAIN,
        transactions.iter().map(|tx| tx.raw.clone()).collect(),
        auth_budget,
        TIMEOUT,
    )?;
    let context = context(root, policy);
    let plan = NovTransferPlan::compile(authenticated, context, policy.clone(), plan_budget)?;
    let commitment = plan.commitment();
    let reads = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let source = Source {
        memory: memory.clone(),
        reads: reads.clone(),
        drops: drops.clone(),
    };
    let input = plan.capture(&source, capture_budget)?;
    let captured_reads = reads.load(Ordering::SeqCst);
    assert!(captured_reads > 0);
    drop(source);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    let actual = input.execute(session, TIMEOUT)?;
    assert_eq!(
        reads.load(Ordering::SeqCst),
        captured_reads,
        "execution must not re-enter parent source"
    );
    assert_eq!(actual.effects().plan_commitment(), commitment);
    assert_eq!(actual.effects().context(), &context);
    assert_eq!(actual.effects().update().parent_root(), root);
    assert_eq!(
        actual.fees(),
        &reference.fees,
        "all money and diagnostics must match independent serial reference"
    );
    assert_eq!(actual.receipts().len(), transactions.len());
    for (receipt, expected) in actual.receipts().iter().zip(&reference.receipts) {
        assert_eq!(receipt.tx_hash, expected.delta.tx_hash);
        assert_eq!(receipt.signer_identity, expected.identity);
        assert_eq!(receipt.delta, expected.delta);
        assert_eq!(receipt.failure, expected.failure);
        assert_eq!(receipt.quote, expected.quote);
        assert_eq!(receipt.fee_failure, expected.fee_failure);
        assert_eq!(receipt.journal, expected.journal);
        assert_eq!(
            receipt.clear_clearing_candidates,
            expected.journal.is_some()
        );
    }
    let mut expected_changes = fee_record_changes(policy, &reference.fees)?;
    expected_changes.extend(
        reference
            .balances
            .iter()
            .map(|(key, value)| put(balance_key(key), value.to_le_bytes().to_vec())),
    );
    expected_changes.extend(
        reference
            .nonces
            .iter()
            .map(|(key, value)| put(nonce_key(key), value.to_le_bytes().to_vec())),
    );
    let expected_root = stage_state_update(&memory, root, &expected_changes)?.root();
    assert_eq!(
        actual.effects().update().root(),
        expected_root,
        "entire output state, including absence/zero and fee pages"
    );
    assert_eq!(
        memory.0, initial_nodes,
        "tentative work cannot mutate parent content"
    );
    let mut result = memory.clone();
    result.0.extend(actual.effects().update().nodes().clone());
    for signed in transactions {
        for bytes in [&signed.tx.from, &signed.tx.to] {
            let key = Account::try_from(bytes.as_slice()).unwrap();
            assert_eq!(
                read_state_value(&result, expected_root, &balance_key(&key))?,
                reference
                    .balances
                    .get(&key)
                    .map(|amount| amount.to_le_bytes().to_vec())
            );
            assert_eq!(
                read_state_value(&result, root, &balance_key(&key))?,
                balances
                    .get(&key)
                    .map(|amount| amount.to_le_bytes().to_vec())
            );
        }
        assert_eq!(
            read_state_value(&result, expected_root, &nonce_key(&signed.identity))?,
            Some(reference.nonces[&signed.identity].to_le_bytes().to_vec())
        );
    }
    assert_ne!(actual.statement_commitment(), [0; 32]);
    assert_ne!(actual.receipt_batch_commitment(), [0; 32]);
    let observation = actual.observation();
    assert!(observation.peak_callbacks >= 1);
    assert!(observation.recomputed_transactions <= transactions.len());
    eprintln!("signed NOV tentative batch: transactions={} components={} credit_only={} recomputed={} actual_peak={} (not TPS/finality)", transactions.len(), observation.components, observation.credit_only_accounts, observation.recomputed_transactions, observation.peak_callbacks);
    Ok(actual)
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; signed economic component only"]
fn real_signed_mixed_batch_matches_independent_serial_economics_after_source_drop() -> Result<()> {
    let mut session = ComputeSession::open(&library(), 4)?;
    let (a, b, c, d, e) = (
        account(1, false),
        account(2, false),
        account(3, false),
        account(4, false),
        account(5, false),
    );
    let mut refused = transfer(1, e.clone(), 5, 2);
    refused.fee_policy.max_pay_amount = 1;
    let mut automatic = transfer(3, e.clone(), 3, 1);
    automatic.asset = " nov ".into();
    automatic.fee_policy.pay_asset = " ".into();
    automatic.fee_policy.slippage_bps = u32::MAX;
    let mut alias = transfer(1, e.clone(), 1, 4);
    alias.from = account(1, true).as_bytes().to_vec();
    let transactions = vec![
        sign(transfer(1, b, 100, 0), 1),
        sign(transfer(2, c.clone(), 20, 0), 2),
        sign(transfer(1, a.clone(), u128::MAX, 1), 1),
        sign(transfer(3, d.clone(), 1, 0), 3),
        sign(refused, 1),
        sign(transfer(1, e.clone(), 0, 3), 1),
        sign(automatic, 3),
        sign(alias, 1),
        sign(transfer(1, e, 1, 5), 1),
    ];
    let balances = BTreeMap::from([(a, 1_000_000), (c, 1_000_000), (d, u128::MAX)]);
    let mut fees = FeeState::default();
    fees.accounting.daily_window_day = 1;
    fees.accounting.daily_nov_used = 777;
    let actual = run_and_check(
        &mut session,
        &transactions,
        &policy(),
        &fees,
        &balances,
        &BTreeMap::new(),
    )?;
    assert_eq!(
        actual
            .receipts()
            .iter()
            .map(|receipt| receipt.failure.is_none())
            .collect::<Vec<_>>(),
        [true, true, false, false, false, true, true, false, true]
    );
    assert_eq!(actual.fees().accounting.settlements, 7);
    assert_eq!(actual.fees().diagnostics.quote_max_pay_exceeded, 1);
    assert_eq!(
        actual.fees().diagnostics.clearing_insufficient_user_balance,
        1
    );
    assert_eq!(actual.fees().accounting.daily_nov_used, 0);
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; no persistent candidate or TPS claim"]
fn real_global_fee_rejections_repair_dependent_predictions_and_shared_credits() -> Result<()> {
    let mut session = ComputeSession::open(&library(), 4)?;
    let sink = account(90, false);
    let transactions = vec![
        sign(transfer(11, account(12, false), 100, 0), 11),
        sign(transfer(11, account(12, false), 20, 1), 11),
        sign(transfer(12, sink.clone(), 10, 0), 12),
        sign(transfer(13, sink.clone(), 7, 0), 13),
        sign(transfer(13, sink.clone(), 8, 1), 13),
    ];
    let balances = BTreeMap::from([(account(11, false), 1000), (account(13, false), 1000)]);
    for paused in [false, true] {
        let mut fees = FeeState::default();
        fees.accounting.settlements = u64::MAX - 1;
        let mut policy = policy();
        policy.settlement_paused = paused;
        policy.resolution_source = "default_fallback_invalid_env".into();
        let actual = run_and_check(
            &mut session,
            &transactions,
            &policy,
            &fees,
            &balances,
            &BTreeMap::new(),
        )?;
        assert!(
            actual.observation().recomputed_transactions > 0,
            "a rejected predicted predecessor must be repaired"
        );
        assert_eq!(actual.fees().diagnostics.settlement_policy_fallback, 5);
        assert!(actual.observation().credit_only_accounts >= 1);
        if paused {
            assert_eq!(actual.fees().diagnostics.settlement_paused, 5);
        } else {
            assert_eq!(actual.fees().accounting.settlements, u64::MAX);
            assert_eq!(actual.fees().diagnostics.settlement_amount_overflow, 4);
        }
    }
    // Pure-credit recipient starts missing; rejected fees must not create its
    // zero balance leaf, even if every component speculated a successful credit.
    let pure: Vec<_> = (21..=24)
        .map(|seed| sign(transfer(seed, sink.clone(), 4, 0), seed))
        .collect();
    let balances = (21..=24).map(|seed| (account(seed, false), 1000)).collect();
    let mut fees = FeeState::default();
    fees.accounting.settlements = u64::MAX;
    let actual = run_and_check(
        &mut session,
        &pure,
        &policy(),
        &fees,
        &balances,
        &BTreeMap::new(),
    )?;
    assert_eq!(actual.observation().components, 4);
    assert_eq!(actual.observation().credit_only_accounts, 1);
    assert!(actual
        .receipts()
        .iter()
        .all(|receipt| receipt.delta.fee_funding_delta == 0));
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; real dependency correction, not just counters"]
fn real_rejected_expensive_fee_turns_a_later_predicted_failure_into_success() -> Result<()> {
    let mut session = ComputeSession::open(&library(), 4)?;
    let recipient = account(89, false);
    let large_amount = 10_u128.pow(28);
    let first = sign(transfer(88, recipient.clone(), large_amount, 0), 88);
    let second = sign(transfer(88, recipient, 100, 1), 88);
    let policy = policy();
    let first_fee = reference_quote(&first.tx, u128::from(NOW), policy.quote_ttl_ms)
        .unwrap()
        .nov_amount;
    let second_fee = reference_quote(&second.tx, u128::from(NOW), policy.quote_ttl_ms)
        .unwrap()
        .nov_amount;
    assert!(
        first_fee > second_fee,
        "fixture needs genuinely different canonical fee amounts"
    );
    let payer_before = large_amount + first_fee + 1;
    let predicted_after_first = payer_before - large_amount - first_fee;
    assert!(
        predicted_after_first < second_fee,
        "old speculation must predict a fee failure"
    );
    let mut fees = FeeState::default();
    // Synthetic extreme accounting fixture: the independent NOV reserve view
    // admits only the cheaper fee. It is not a proposed genesis allocation.
    fees.accounting.treasury_reserve_nov = Some(u128::MAX - second_fee);
    let balances = BTreeMap::from([(account(88, false), payer_before)]);
    let actual = run_and_check(
        &mut session,
        &[first, second],
        &policy,
        &fees,
        &balances,
        &BTreeMap::new(),
    )?;
    assert_eq!(
        actual.receipts()[0].fee_failure.as_ref().unwrap().code,
        FeeFailureCode::AmountOverflow
    );
    assert_eq!(actual.receipts()[0].delta.fee_funding_delta, 0);
    assert!(
        actual.receipts()[1].failure.is_none(),
        "later speculative failure MUST be recomputed from the actual preserved balance"
    );
    assert_eq!(actual.receipts()[1].delta.payer.before, payer_before);
    assert_eq!(
        actual.receipts()[1].delta.payer.after,
        payer_before - second_fee - 100
    );
    assert_eq!(
        actual.fees().accounting.treasury_reserve_nov,
        Some(u128::MAX)
    );
    assert_eq!(actual.fees().accounting.settlements, 1);
    assert!(actual.observation().recomputed_transactions >= 1);
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; natural overlap, no sleep/barrier"]
fn real_independent_shared_credit_components_have_identical_complete_effects() -> Result<()> {
    let mut session = ComputeSession::open(&library(), 8)?;
    let sink = account(110, false);
    let transactions: Vec<_> = (30..94)
        .map(|seed| sign(transfer(seed, sink.clone(), 1, 0), seed))
        .collect();
    let balances = (30..94).map(|seed| (account(seed, false), 1000)).collect();
    let mut policy = policy();
    // These are non-NOV gates; direct NOV remains payable, with Blocked metadata.
    policy.clearing_enabled = false;
    policy.clearing_require_healthy_risk_buffer = true;
    policy.min_risk_buffer_nov = u128::MAX;
    let one = run_and_check(
        &mut session,
        &transactions,
        &policy,
        &FeeState::default(),
        &balances,
        &BTreeMap::new(),
    )?;
    assert_eq!(one.observation().components, 64);
    assert_eq!(one.observation().credit_only_accounts, 1);
    assert!(one
        .receipts()
        .iter()
        .all(|receipt| receipt.failure.is_none()));
    assert!(one.receipts().iter().all(|receipt| receipt
        .journal
        .as_ref()
        .unwrap()
        .policy_threshold_state
        == ThresholdState::Blocked));
    let two = run_and_check(
        &mut session,
        &transactions,
        &policy,
        &FeeState::default(),
        &balances,
        &BTreeMap::new(),
    )?;
    assert_eq!(one.effects().update().root(), two.effects().update().root());
    assert_eq!(one.statement_commitment(), two.statement_commitment());
    assert_eq!(
        one.receipt_batch_commitment(),
        two.receipt_batch_commitment()
    );
    assert!(
        one.observation()
            .peak_callbacks
            .max(two.observation().peak_callbacks)
            > 1,
        "this explicit multicore AOEM gate requires observed overlap; no sleep/barrier creates it"
    );
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; checked credit fallback, not TPS"]
fn real_shared_credit_headroom_overflow_keeps_ordered_conflicts_and_fees() -> Result<()> {
    let mut session = ComputeSession::open(&library(), 4)?;
    let recipient = account(120, false);
    let transactions: Vec<_> = (111..115)
        .map(|seed| sign(transfer(seed, recipient.clone(), 2, 0), seed))
        .collect();
    let mut balances: BTreeMap<_, _> = (111..115)
        .map(|seed| (account(seed, false), 1000))
        .collect();
    // Every sender is independent, but the aggregate recipient headroom is
    // insufficient. Sharing a receive-only address must NOT erase this real
    // order-dependent overflow: only the first credit succeeds, while all four
    // transactions still pay fees and consume their authenticated signer nonce.
    balances.insert(recipient, u128::MAX - 3);
    let actual = run_and_check(
        &mut session,
        &transactions,
        &policy(),
        &FeeState::default(),
        &balances,
        &BTreeMap::new(),
    )?;
    assert_eq!(actual.observation().components, 1);
    assert_eq!(actual.observation().credit_only_accounts, 0);
    assert!(actual.receipts()[0].failure.is_none());
    assert_eq!(actual.receipts()[0].delta.recipient.after, u128::MAX - 1);
    for receipt in &actual.receipts()[1..] {
        assert_eq!(
            receipt.failure,
            Some(TransferFailure::Business(TransferError::RecipientOverflow))
        );
        assert_eq!(receipt.delta.recipient.before, u128::MAX - 1);
        assert_eq!(receipt.delta.recipient.after, u128::MAX - 1);
    }
    assert!(actual
        .receipts()
        .iter()
        .all(|receipt| { receipt.delta.fee_funding_delta > 0 && receipt.delta.nonce_after == 1 }));
    assert_eq!(actual.fees().accounting.settlements, 4);
    Ok(())
}

#[test]
#[ignore = "requires explicit NOVOVM_AOEM_TEST_LIBRARY; rejection has no staged accepted output"]
fn exact_policy_root_and_nonce_inputs_cannot_be_substituted() -> Result<()> {
    let mut session = ComputeSession::open(&library(), 4)?;
    let policy = policy();
    let valid = sign(transfer(101, account(102, false), 1, 0), 101);
    let balances = BTreeMap::from([(account(101, false), 1000)]);
    let (memory, root) = initial_tree(&policy, &FeeState::default(), &balances, &BTreeMap::new())?;
    for case in [
        "policy",
        "root",
        "nonce",
        "program",
        "contract",
        "semantic",
        "receipt",
        "asset",
        "fee_asset",
    ] {
        let signed = if case == "nonce" {
            sign(transfer(101, account(102, false), 1, 1), 101)
        } else if case == "asset" {
            let mut tx = transfer(101, account(102, false), 1, 0);
            tx.asset = "USDT".into();
            sign(tx, 101)
        } else if case == "fee_asset" {
            let mut tx = transfer(101, account(102, false), 1, 0);
            tx.fee_policy.pay_asset = "USDT".into();
            sign(tx, 101)
        } else {
            sign(valid.tx.clone(), 101)
        };
        let (auth, plan, capture) = budgets();
        let batch = authenticate_batch(&mut session, CHAIN, vec![signed.raw], auth, TIMEOUT)?;
        let mut claimed_policy = policy.clone();
        if case == "policy" {
            claimed_policy.quote_ttl_ms += 1;
        }
        let mut context = context(root, &claimed_policy);
        if case == "root" {
            context.parent_state_root = [0x78; 32];
        }
        if case == "program" {
            context.business_program = [0x79; 32];
        }
        if case == "contract" {
            context.effect_contract = [0x7a; 32];
        }
        if case == "semantic" {
            context.semantic_version += 1;
        }
        if case == "receipt" {
            context.receipt_codec = [0x7b; 32];
        }
        let planned = NovTransferPlan::compile(batch, context, claimed_policy, plan);
        if matches!(
            case,
            "program" | "contract" | "semantic" | "receipt" | "asset" | "fee_asset"
        ) {
            assert!(planned.is_err(), "{case}");
        } else {
            assert!(planned?.capture(&memory, capture).is_err(), "{case}");
        }
    }
    // Bad inputs rejected before business do not kill the shared compute owner.
    run_and_check(
        &mut session,
        &[valid],
        &policy,
        &FeeState::default(),
        &balances,
        &BTreeMap::new(),
    )?;
    Ok(())
}
