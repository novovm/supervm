//! Fixed native NOV supply and candidate monetary-integrity checks.
//!
//! This is not another token runtime or balance store. Genesis installs the
//! original account leaves; both execution backends use the same checked output
//! boundary before staging those leaves and the original fee record. Permission
//! budgets, lock views, fee diagnostics and cumulative turnover are NOT assets.
use super::{direct_nov_fee::FeeState, quoted_transfer::Account};
use anyhow::{ensure, Context, Result};
use std::collections::BTreeMap;

pub const NOV_DECIMALS: u8 = 9;
pub const NOV_BASE_UNITS_PER_TOKEN: u128 = 1_000_000_000;
pub const NOV_GENESIS_TOKENS: u128 = 100_000_000;
pub const NOV_FIXED_SUPPLY_BASE_UNITS: u128 = NOV_GENESIS_TOKENS * NOV_BASE_UNITS_PER_TOKEN;

/// Original genesis preparation calls this after canonical amount/address
/// validation and deduplication. There is no test-network supply bypass here.
/// Allocation ownership, restrictions and approval remain the manifest owner's
/// responsibility; a correct sum alone never authorizes creation or release.
pub fn validate_genesis_supply(amounts: impl IntoIterator<Item = u128>) -> Result<()> {
    let mut total = 0u128;
    for amount in amounts {
        ensure!(amount != 0, "NOV genesis allocation must be positive");
        total = total
            .checked_add(amount)
            .context("NOV genesis supply overflow")?;
        ensure!(
            total <= NOV_FIXED_SUPPLY_BASE_UNITS,
            "NOV genesis exceeds fixed supply of 100000000000000000 base units"
        );
    }
    ensure!(
        total == NOV_FIXED_SUPPLY_BASE_UNITS,
        "NOV genesis must issue exactly 100000000000000000 base units (100000000 NOV, 9 decimals)"
    );
    Ok(())
}

/// Exact sum for output verification, not a widened business amount. This also
/// supports retained synthetic overflow fixtures whose independent balances sum
/// beyond u128; no modulo/saturating sum may hide a mint or burn in those tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ExactTotal {
    high: u128,
    low: u128,
}

impl ExactTotal {
    fn add(&mut self, value: u128) -> Result<()> {
        let (low, carry) = self.low.overflowing_add(value);
        self.high = self
            .high
            .checked_add(u128::from(carry))
            .context("NOV integrity sum overflow")?;
        self.low = low;
        Ok(())
    }
}

/// Checks the complete captured balance projection and final executor output,
/// never prediction balances or a caller-provided authorization snapshot.
///
/// The original treasury_reserve_nov is the one balance holding collected fees.
/// Its reserve/fee/risk buckets are disjoint subdivisions; cumulative settled
/// totals and daily usage are overlapping views. Count treasury money ONCE.
/// Reject a bad executor result, do not repair it or decide any transaction.
pub(crate) fn validate_candidate_conservation(
    before: &BTreeMap<Account, Option<u128>>,
    after: &BTreeMap<Account, Option<u128>>,
    fees_before: &FeeState,
    fees_after: &FeeState,
    charged_fees: impl IntoIterator<Item = u128>,
) -> Result<()> {
    ensure!(
        before.keys().eq(after.keys()),
        "NOV result account set differs from captured parent"
    );
    let mut parent_money = ExactTotal::default();
    let mut result_money = ExactTotal::default();
    for (account, old) in before {
        let new = after[account];
        ensure!(
            old.is_none() || new.is_some(),
            "NOV result removed an existing account balance"
        );
        parent_money.add(old.unwrap_or(0))?;
        result_money.add(new.unwrap_or(0))?;
    }
    let old = &fees_before.accounting;
    let new = &fees_after.accounting;
    parent_money.add(old.treasury_reserve_nov.unwrap_or(0))?;
    result_money.add(new.treasury_reserve_nov.unwrap_or(0))?;
    ensure!(
        parent_money == result_money,
        "NOV candidate would mint or burn native assets"
    );

    let charged = charged_fees.into_iter().try_fold(0u128, |sum, fee| {
        sum.checked_add(fee).context("NOV charged fee sum overflow")
    })?;
    let delta = |next: u128, previous: u128| {
        next.checked_sub(previous)
            .context("NOV transfer decreased fee accounting")
    };
    ensure!(
        delta(
            new.treasury_reserve_nov.unwrap_or(0),
            old.treasury_reserve_nov.unwrap_or(0)
        )? == charged
            && delta(new.settled_nov_total, old.settled_nov_total)? == charged
            && delta(
                new.settled_by_asset_nov.unwrap_or(0),
                old.settled_by_asset_nov.unwrap_or(0)
            )? == charged,
        "NOV fee assets, cumulative views and receipts disagree"
    );
    let mut bucket_delta = ExactTotal::default();
    bucket_delta.add(delta(new.reserve_bucket_nov, old.reserve_bucket_nov)?)?;
    bucket_delta.add(delta(new.fee_bucket_nov, old.fee_bucket_nov)?)?;
    bucket_delta.add(delta(new.risk_buffer_nov, old.risk_buffer_nov)?)?;
    ensure!(
        bucket_delta
            == ExactTotal {
                high: 0,
                low: charged
            },
        "NOV fee bucket deltas do not conserve charged assets"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn balances(a: u128, b: u128) -> BTreeMap<Account, Option<u128>> {
        BTreeMap::from([
            (Account::try_from(vec![1; 32]).unwrap(), Some(a)),
            (Account::try_from(vec![2; 32]).unwrap(), Some(b)),
        ])
    }

    fn credited_fee(amount: u128) -> FeeState {
        let mut state = FeeState::default();
        state.accounting.treasury_reserve_nov = Some(amount);
        state.accounting.settled_nov_total = amount;
        state.accounting.settled_by_asset_nov = Some(amount);
        state.accounting.reserve_bucket_nov = amount / 3;
        state.accounting.fee_bucket_nov = amount / 3;
        state.accounting.risk_buffer_nov = amount - 2 * (amount / 3);
        state
    }

    #[test]
    fn fixed_genesis_supply_has_no_mint_burn_or_precision_override() {
        assert_eq!(NOV_DECIMALS, 9);
        validate_genesis_supply([NOV_FIXED_SUPPLY_BASE_UNITS]).unwrap();
        validate_genesis_supply([1, NOV_FIXED_SUPPLY_BASE_UNITS - 1]).unwrap();
        for values in [
            vec![],
            vec![0],
            vec![NOV_FIXED_SUPPLY_BASE_UNITS - 1],
            vec![NOV_FIXED_SUPPLY_BASE_UNITS, 1],
            vec![u128::MAX],
        ] {
            assert!(validate_genesis_supply(values).is_err());
        }
    }

    #[test]
    fn transfer_and_fee_count_single_owner_not_accounting_views() {
        let before = balances(1000, 0);
        let after = balances(850, 100);
        validate_candidate_conservation(
            &before,
            &after,
            &FeeState::default(),
            &credited_fee(50),
            [50],
        )
        .unwrap();
    }

    #[test]
    fn shared_receiver_and_self_transfer_do_not_duplicate_asset_count() {
        let before = balances(1000, 0);
        // Two transfers to one recipient: all accounts appear exactly once.
        validate_candidate_conservation(
            &before,
            &balances(800, 100),
            &FeeState::default(),
            &credited_fee(100),
            [50, 50],
        )
        .unwrap();
        // A self-transfer changes only the fee, not the account's gross amount.
        validate_candidate_conservation(
            &before,
            &balances(950, 0),
            &FeeState::default(),
            &credited_fee(50),
            [50],
        )
        .unwrap();
    }

    #[test]
    fn failures_may_keep_diagnostics_but_cannot_hide_balance_changes() {
        let before = balances(1000, 0);
        let mut after_fee = FeeState::default();
        after_fee.accounting.daily_window_day = 99;
        after_fee.diagnostics.settlement_paused = 1;
        validate_candidate_conservation(&before, &before, &FeeState::default(), &after_fee, [])
            .unwrap();
        assert!(validate_candidate_conservation(
            &before,
            &balances(999, 0),
            &FeeState::default(),
            &after_fee,
            []
        )
        .is_err());
        // Business failure may charge its original actual fee, but no principal.
        validate_candidate_conservation(
            &before,
            &balances(950, 0),
            &FeeState::default(),
            &credited_fee(50),
            [50],
        )
        .unwrap();
    }

    #[test]
    fn minted_burned_or_wrong_fee_outputs_are_rejected() {
        let before = balances(1000, 0);
        for after in [balances(851, 100), balances(849, 100)] {
            assert!(validate_candidate_conservation(
                &before,
                &after,
                &FeeState::default(),
                &credited_fee(50),
                [50]
            )
            .is_err());
        }
        for alter in [0, 1, 2] {
            let mut fees = credited_fee(50);
            match alter {
                0 => fees.accounting.settled_nov_total += 1,
                1 => fees.accounting.fee_bucket_nov += 1,
                _ => fees.accounting.settled_by_asset_nov = None,
            }
            assert!(validate_candidate_conservation(
                &before,
                &balances(850, 100),
                &FeeState::default(),
                &fees,
                [50]
            )
            .is_err());
        }
        assert!(validate_candidate_conservation(
            &before,
            &balances(850, 100),
            &FeeState::default(),
            &credited_fee(50),
            [49]
        )
        .is_err());
    }

    #[test]
    fn missing_extra_or_removed_account_outputs_are_rejected() {
        let before = balances(1000, 0);
        let mut missing = before.clone();
        missing.pop_last();
        let mut extra = before.clone();
        extra.insert(Account::try_from(vec![3; 32]).unwrap(), Some(0));
        let mut removed = before.clone();
        *removed.last_entry().unwrap().get_mut() = None;
        for after in [missing, extra, removed] {
            assert!(validate_candidate_conservation(
                &before,
                &after,
                &FeeState::default(),
                &FeeState::default(),
                []
            )
            .is_err());
        }
    }

    #[test]
    fn exact_totals_do_not_wrap_at_u128_boundary() {
        let before = balances(u128::MAX, u128::MAX);
        validate_candidate_conservation(
            &before,
            &before,
            &FeeState::default(),
            &FeeState::default(),
            [],
        )
        .unwrap();
        assert!(validate_candidate_conservation(
            &before,
            &balances(u128::MAX - 1, u128::MAX),
            &FeeState::default(),
            &FeeState::default(),
            []
        )
        .is_err());
        let mut total = ExactTotal::default();
        total.add(u128::MAX).unwrap();
        total.add(1).unwrap();
        assert_eq!(total, ExactTotal { high: 1, low: 0 });
    }

    #[test]
    fn snapshot_codec_does_not_turn_cumulative_views_into_assets() {
        let old = credited_fee(50);
        let mut after = credited_fee(100);
        after.accounting.daily_window_day = 1;
        after.accounting.daily_nov_used = 0;
        let bytes = postcard::to_allocvec(&after).unwrap();
        let restored: FeeState = postcard::from_bytes(&bytes).unwrap();
        validate_candidate_conservation(
            &balances(1000, 0),
            &balances(850, 100),
            &old,
            &restored,
            [50],
        )
        .unwrap();
    }
}
