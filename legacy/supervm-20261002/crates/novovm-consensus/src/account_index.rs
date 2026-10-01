use crate::token_runtime::Web30TokenRuntime;
use web30_core::types::Address as Web30Address;

/// Cross-module unified account index (consensus deterministic snapshot service).
#[derive(Debug, Clone)]
pub struct UnifiedAccountIndex {
    dividend_min_balance: u128,
    dividend_balances: Vec<(Web30Address, u128)>,
    version: u64,
}

impl UnifiedAccountIndex {
    pub fn new(dividend_min_balance: u128) -> Self {
        Self {
            dividend_min_balance,
            dividend_balances: Vec::new(),
            version: 0,
        }
    }

    pub fn refresh_from_token_runtime(&mut self, token_runtime: &Web30TokenRuntime) -> usize {
        let latest = token_runtime.dividend_eligible_balances(self.dividend_min_balance);
        if latest != self.dividend_balances {
            self.version = self.version.saturating_add(1);
            self.dividend_balances = latest;
        }
        self.dividend_balances.len()
    }

    pub fn dividend_snapshot(&self) -> Vec<(Web30Address, u128)> {
        self.dividend_balances.clone()
    }

    pub fn version(&self) -> u64 {
        self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TokenEconomicsPolicy;
    use std::time::Instant;

    #[test]
    fn test_unified_account_index_refresh_is_sorted_and_versioned() {
        let policy = TokenEconomicsPolicy::default();
        let mut runtime = Web30TokenRuntime::from_policy(&policy).expect("runtime init");
        runtime.mint(7, 500).expect("mint 7");
        runtime.mint(2, 500).expect("mint 2");

        let mut index = UnifiedAccountIndex::new(100);
        let count = index.refresh_from_token_runtime(&runtime);
        assert!(count >= 2);
        assert_eq!(index.version(), 1);

        let snapshot = index.dividend_snapshot();
        assert!(snapshot
            .windows(2)
            .all(|w| w[0].0.as_bytes() <= w[1].0.as_bytes()));

        // Same state should not bump version.
        index.refresh_from_token_runtime(&runtime);
        assert_eq!(index.version(), 1);
    }

    #[test]
    fn test_unified_account_index_refresh_large_scale_perf_budget() {
        let account_count: usize = std::env::var("NOVOVM_ACCOUNT_INDEX_PERF_ACCOUNTS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(20_000);
        let max_ms: u128 = std::env::var("NOVOVM_ACCOUNT_INDEX_PERF_MAX_MS")
            .ok()
            .and_then(|v| v.parse::<u128>().ok())
            .unwrap_or(8_000);

        let policy = TokenEconomicsPolicy::default();
        let mut runtime = Web30TokenRuntime::from_policy(&policy).expect("runtime init");
        for i in 1..=account_count {
            runtime
                .mint(i as u32, 500)
                .expect("seed account balance for perf test");
        }

        let mut index = UnifiedAccountIndex::new(100);
        let t0 = Instant::now();
        let count = index.refresh_from_token_runtime(&runtime);
        let elapsed_ms = t0.elapsed().as_millis();

        assert_eq!(count, account_count, "snapshot account count mismatch");
        assert!(
            elapsed_ms <= max_ms,
            "account index refresh performance budget exceeded: elapsed_ms={} max_ms={} accounts={}",
            elapsed_ms,
            max_ms,
            account_count
        );
    }
}
