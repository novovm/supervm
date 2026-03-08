//! Dividend Pool - daily active claim mechanism
//!
//! Design highlights:
//! - Daily snapshot at UTC 00:00
//! - Users actively call claim() to receive dividends
//! - Unclaimed dividends accumulate automatically
//! - Anti-dust protection (minimum holding 100 tokens)

use crate::types::Address;
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// Daily dividend snapshot
#[derive(Debug, Clone)]
pub struct DailySnapshot {
    /// Snapshot day (Unix days, day = timestamp / 86400)
    pub day: u64,
    /// Pool income of the day
    pub pool_income: u128,
    /// Total circulating supply at snapshot
    pub total_circulating: u128,
    /// User balance snapshot (address -> balance)
    pub balances: HashMap<Address, u128>,
}

/// Dividend claim record
#[derive(Debug, Clone)]
pub struct ClaimRecord {
    /// Claimer address
    pub claimer: Address,
    /// Claim day
    pub day: u64,
    /// Claimed amount
    pub amount: u128,
    /// Claim timestamp
    pub timestamp: u64,
}

/// Dividend pool events
#[derive(Debug, Clone)]
pub enum DividendEvent {
    /// Snapshot created
    SnapshotCreated {
        day: u64,
        pool_income: u128,
        total_circulating: u128,
        eligible_holders: usize,
    },
    /// Dividends claimed
    Claimed {
        claimer: Address,
        day: u64,
        amount: u128,
        cumulative_days: u64, // accumulated days
    },
    /// Income received
    IncomeReceived {
        from: Address,
        amount: u128,
        current_day: u64,
    },
}

/// Dividend pool interface
pub trait DividendPool {
    /// Get current day (Unix days)
    fn current_day(&self) -> u64;

    /// Take daily snapshot (triggered by cron/first txn)
    ///
    /// Process:
    /// 1. Check if today's snapshot exists
    /// 2. Iterate holders with balance > 100 tokens
    /// 3. Record balances and total circulating supply
    /// 4. Attach yesterday's pool income
    fn take_daily_snapshot(&mut self) -> Result<DividendEvent>;

    /// Calculate user's claimable accumulated dividends
    ///
    /// Formula:
    /// claimable = Σ (user_balance[day] / total_supply[day] × pool_income[day])
    ///           for day in [last_claimed_day + 1, current_day - 1]
    ///
    /// Args:
    /// - user: address
    ///
    /// Returns: (claimable, accumulated days)
    fn get_claimable(&self, user: &Address) -> Result<(u128, u64)>;

    /// Claim dividends
    ///
    /// Constraints:
    /// - At most once per day
    /// - Minimum holding 100 tokens
    /// - At least 1 day of unclaimed dividends
    ///
    /// Returns: claim event
    fn claim(&mut self, user: &Address) -> Result<DividendEvent>;

    /// Record pool income (invoked by MainnetToken fee routing)
    fn receive_income(&mut self, from: &Address, amount: u128) -> Result<DividendEvent>;

    /// Get user's last claimed day
    fn last_claimed_day(&self, user: &Address) -> Option<u64>;

    /// Get snapshot by day
    fn get_snapshot(&self, day: u64) -> Option<&DailySnapshot>;

    /// Get total income
    fn total_income(&self) -> u128;

    /// Get total claimed amount
    fn total_claimed(&self) -> u128;

    /// Get current pool balance
    fn pool_balance(&self) -> u128 {
        self.total_income().saturating_sub(self.total_claimed())
    }
}

/// Dividend pool implementation
pub struct DividendPoolImpl {
    /// Snapshot history (day -> snapshot)
    snapshots: HashMap<u64, DailySnapshot>,
    /// User last claimed day (user -> day)
    last_claimed: HashMap<Address, u64>,
    /// Claim history (for auditing)
    claim_history: Vec<ClaimRecord>,
    /// Total income
    total_income: u128,
    /// Total claimed
    total_claimed: u128,
    /// Current unallocated balance
    current_pool: u128,
    /// Minimum balance requirement (anti-dust)
    min_balance: u128,
    /// Account balance snapshot source (injected by runtime/token layer)
    account_balances: HashMap<Address, u128>,
    /// Reentrancy guard for claim path
    claim_in_progress: bool,
}

impl DividendPoolImpl {
    pub fn new(min_balance: u128) -> Self {
        Self {
            snapshots: HashMap::new(),
            last_claimed: HashMap::new(),
            claim_history: Vec::new(),
            total_income: 0,
            total_claimed: 0,
            current_pool: 0,
            min_balance,
            account_balances: HashMap::new(),
            claim_in_progress: false,
        }
    }

    /// Inject a full account-balance snapshot from runtime/token layer.
    /// This keeps dividend module decoupled from token storage internals.
    pub fn set_account_balances<I>(&mut self, balances: I)
    where
        I: IntoIterator<Item = (Address, u128)>,
    {
        self.account_balances.clear();
        for (addr, amount) in balances {
            self.account_balances.insert(addr, amount);
        }
    }

    /// Set or update one account balance.
    pub fn set_account_balance(&mut self, addr: Address, amount: u128) {
        self.account_balances.insert(addr, amount);
    }

    /// Clear injected account balances.
    pub fn clear_account_balances(&mut self) {
        self.account_balances.clear();
    }

    /// Internal helper: get current timestamp (seconds)
    fn now(&self) -> u64 {
        // In production, fetch from VM/Runtime block timestamp
        // Example only
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn checked_add_u128(lhs: u128, rhs: u128, ctx: &str) -> Result<u128> {
        lhs.checked_add(rhs)
            .ok_or_else(|| anyhow!("overflow in {}", ctx))
    }
}

impl DividendPool for DividendPoolImpl {
    fn current_day(&self) -> u64 {
        self.now() / 86400
    }

    fn take_daily_snapshot(&mut self) -> Result<DividendEvent> {
        let today = self.current_day();

        // Check if snapshot for today exists
        if self.snapshots.contains_key(&today) {
            return Err(anyhow!("Today's snapshot already exists"));
        }

        // Get yesterday's income (attach to today's snapshot)
        let yesterday_income = self.current_pool;

        // In production, fetch all balances from MainnetToken
        // Simplified here to external-provided balances HashMap
        let balances = self.collect_eligible_balances()?;
        let total_circulating: u128 = balances.values().sum();

        let snapshot = DailySnapshot {
            day: today,
            pool_income: yesterday_income,
            total_circulating,
            balances: balances.clone(),
        };

        let eligible_holders = balances.len();
        self.snapshots.insert(today, snapshot);

        // Reset today's income accumulator
        self.current_pool = 0;

        Ok(DividendEvent::SnapshotCreated {
            day: today,
            pool_income: yesterday_income,
            total_circulating,
            eligible_holders,
        })
    }

    fn get_claimable(&self, user: &Address) -> Result<(u128, u64)> {
        let today = self.current_day();
        let last_claimed = self.last_claimed.get(user).copied().unwrap_or(0);

        // Claimable range: [last_claimed + 1, today]
        // Runtime is expected to call snapshot before claim flow in the same day window.
        let mut total_claimable = 0u128;
        let mut days_count = 0u64;

        for day in (last_claimed + 1)..=today {
            if let Some(snapshot) = self.snapshots.get(&day) {
                if snapshot.total_circulating == 0 {
                    continue;
                }
                if let Some(&user_balance) = snapshot.balances.get(user) {
                    if user_balance >= self.min_balance {
                        // Compute daily share: (user_balance / total) × income
                        let user_share =
                            (user_balance * snapshot.pool_income) / snapshot.total_circulating;
                        total_claimable =
                            Self::checked_add_u128(total_claimable, user_share, "claimable sum")?;
                        days_count += 1;
                    }
                }
            }
        }

        Ok((total_claimable, days_count))
    }

    fn claim(&mut self, user: &Address) -> Result<DividendEvent> {
        if self.claim_in_progress {
            return Err(anyhow!("Reentrant claim blocked"));
        }

        let (claimable, cumulative_days) = self.get_claimable(user)?;

        if claimable == 0 {
            return Err(anyhow!("No claimable dividends"));
        }

        let today = self.current_day();

        // Check if already claimed today (reentrancy guard)
        if let Some(&last_day) = self.last_claimed.get(user) {
            if last_day == today {
                return Err(anyhow!("Already claimed today"));
            }
        }

        self.claim_in_progress = true;
        let result = {
            // Effects before interactions (CEI).
            self.last_claimed.insert(*user, today);
            self.total_claimed =
                Self::checked_add_u128(self.total_claimed, claimable, "total_claimed + claimable")?;
            self.claim_history.push(ClaimRecord {
                claimer: *user,
                day: today,
                amount: claimable,
                timestamp: self.now(),
            });

            // In production, call MainnetToken.transfer to pay user.
            // Keep this after state updates to avoid reentrancy double-claim.
            // transfer(dividend_pool_address, user, claimable)?;

            Ok(DividendEvent::Claimed {
                claimer: *user,
                day: today,
                amount: claimable,
                cumulative_days,
            })
        };
        self.claim_in_progress = false;
        result
    }

    fn receive_income(&mut self, from: &Address, amount: u128) -> Result<DividendEvent> {
        self.total_income =
            Self::checked_add_u128(self.total_income, amount, "total_income + amount")?;
        self.current_pool =
            Self::checked_add_u128(self.current_pool, amount, "current_pool + amount")?;

        Ok(DividendEvent::IncomeReceived {
            from: *from,
            amount,
            current_day: self.current_day(),
        })
    }

    fn last_claimed_day(&self, user: &Address) -> Option<u64> {
        self.last_claimed.get(user).copied()
    }

    fn get_snapshot(&self, day: u64) -> Option<&DailySnapshot> {
        self.snapshots.get(&day)
    }

    fn total_income(&self) -> u128 {
        self.total_income
    }

    fn total_claimed(&self) -> u128 {
        self.total_claimed
    }
}

impl DividendPoolImpl {
    /// Internal helper: collect all eligible balances
    /// In production, read from MainnetToken state
    fn collect_eligible_balances(&self) -> Result<HashMap<Address, u128>> {
        let eligible: HashMap<Address, u128> = self
            .account_balances
            .iter()
            .filter_map(|(addr, amount)| {
                if *amount >= self.min_balance {
                    Some((*addr, *amount))
                } else {
                    None
                }
            })
            .collect();

        if eligible.is_empty() {
            return Err(anyhow!("No eligible holders for snapshot"));
        }

        Ok(eligible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dividend_pool_basic() {
        let mut pool = DividendPoolImpl::new(100);

        // Simulate income
        let treasury = Address::from_bytes([1u8; 32]);
        let income_event = pool.receive_income(&treasury, 10000).unwrap();

        match income_event {
            DividendEvent::IncomeReceived { amount, .. } => {
                assert_eq!(amount, 10000);
            }
            _ => panic!("Unexpected event"),
        }

        assert_eq!(pool.pool_balance(), 10000);
    }

    #[test]
    fn test_claimable_calculation() {
        // Requires a full integration environment
        // including MainnetToken balance state
    }

    #[test]
    fn test_claim_reentrancy_guard_blocks_nested_entry() {
        let mut pool = DividendPoolImpl::new(100);
        pool.claim_in_progress = true;
        let user = Address::from_bytes([2u8; 32]);
        let err = pool.claim(&user).unwrap_err().to_string();
        assert!(err.contains("Reentrant claim blocked"));
    }

    #[test]
    fn test_snapshot_and_claim_with_injected_balances() {
        let mut pool = DividendPoolImpl::new(100);
        let treasury = Address::from_bytes([1u8; 32]);
        let user = Address::from_bytes([2u8; 32]);
        let other = Address::from_bytes([3u8; 32]);

        pool.set_account_balance(user, 1_000);
        pool.set_account_balance(other, 1_000);
        pool.receive_income(&treasury, 10_000).expect("income");
        pool.take_daily_snapshot().expect("snapshot");

        let event = pool.claim(&user).expect("claim");
        match event {
            DividendEvent::Claimed { amount, .. } => {
                assert_eq!(amount, 5_000);
            }
            _ => panic!("Unexpected event"),
        }
    }
}
