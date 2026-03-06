//! NAV rigid redemption module
//!
//! Key components of dual-track pricing:
//! - Daily NAV calculation (reserve assets + treasury balance) / M1
//! - T+7 redemption queue management
//! - Daily redemption quota control
//! - Reserve threshold check (pause redemption if < 50%)

use std::collections::HashMap;

use anyhow::{bail, Result};

use crate::types::Address;

/// NAV snapshot
#[derive(Debug, Clone)]
pub struct NavSnapshot {
    /// Snapshot day (Unix days)
    pub day: u64,
    /// NAV value (stablecoin smallest unit, e.g., USDT 1e-6)
    pub nav_value: u128,
    /// Total reserve asset value
    pub reserve_value: u128,
    /// Treasury balance
    pub treasury_balance: u128,
    /// M1 circulating supply
    pub circulating_supply: u128,
}

/// Redemption request
#[derive(Debug, Clone)]
pub struct RedemptionRequest {
    /// Request ID
    pub request_id: [u8; 32],
    /// Requester
    pub requester: Address,
    /// Token amount to redeem
    pub token_amount: u128,
    /// NAV at request time
    pub nav_at_request: u128,
    /// Expected redemption (stablecoin)
    pub expected_stable: u128,
    /// Request day
    pub request_day: u64,
    /// Execution day (T+7)
    pub execution_day: u64,
    /// Status
    pub status: RedemptionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedemptionStatus {
    Pending,   // waiting
    Executed,  // executed
    Cancelled, // cancelled
}

/// NAV rigid redemption manager
pub struct NavRedemptionManager {
    /// NAV history snapshots
    nav_history: HashMap<u64, NavSnapshot>,
    /// Redemption queue (day -> requests)
    redemption_queue: HashMap<u64, Vec<RedemptionRequest>>,
    /// Daily redemption quota (stablecoin amount)
    daily_quota: u128,
    /// Used quota (day -> used)
    quota_used: HashMap<u64, u128>,
    /// Minimum reserve ratio (bps, 5000 = 50%)
    min_reserve_ratio_bp: u16,
    /// T+N settlement days
    settlement_days: u64,
    /// Total redeemed stats
    total_redeemed: u128,
}

impl NavRedemptionManager {
    pub fn new(daily_quota: u128, min_reserve_ratio_bp: u16, settlement_days: u64) -> Self {
        Self {
            nav_history: HashMap::new(),
            redemption_queue: HashMap::new(),
            daily_quota,
            quota_used: HashMap::new(),
            min_reserve_ratio_bp,
            settlement_days,
            total_redeemed: 0,
        }
    }

    /// Record daily NAV snapshot
    pub fn record_nav_snapshot(
        &mut self,
        day: u64,
        reserve_value: u128,
        treasury_balance: u128,
        circulating_supply: u128,
    ) -> Result<NavSnapshot> {
        if circulating_supply == 0 {
            bail!("Circulating supply must be > 0");
        }

        // NAV = (reserve + treasury) / M1
        let total_value = reserve_value.saturating_add(treasury_balance);
        let nav_value = total_value * 1_000_000 / circulating_supply; // 归一化到 1e6

        let snapshot = NavSnapshot {
            day,
            nav_value,
            reserve_value,
            treasury_balance,
            circulating_supply,
        };

        self.nav_history.insert(day, snapshot.clone());
        Ok(snapshot)
    }

    /// Get NAV by day
    pub fn get_nav(&self, day: u64) -> Option<&NavSnapshot> {
        self.nav_history.get(&day)
    }

    /// Get latest NAV
    pub fn latest_nav(&self) -> Option<&NavSnapshot> {
        self.nav_history
            .iter()
            .max_by_key(|(day, _)| *day)
            .map(|(_, snapshot)| snapshot)
    }

    /// Check if reserve ratio meets redemption condition
    pub fn check_reserve_ratio(&self, day: u64) -> Result<bool> {
        let snapshot = self
            .get_nav(day)
            .ok_or_else(|| anyhow::anyhow!("NAV snapshot not found for day {}", day))?;

        if snapshot.reserve_value == 0 && snapshot.treasury_balance == 0 {
            return Ok(false);
        }

        let total_value = snapshot
            .reserve_value
            .saturating_add(snapshot.treasury_balance);
        let reserve_ratio = (snapshot.reserve_value as f64 / total_value as f64 * 10_000.0) as u16;

        Ok(reserve_ratio >= self.min_reserve_ratio_bp)
    }

    /// Submit redemption request
    pub fn submit_redemption(
        &mut self,
        request_id: [u8; 32],
        requester: Address,
        token_amount: u128,
        current_day: u64,
    ) -> Result<RedemptionRequest> {
        if token_amount == 0 {
            bail!("Token amount must be > 0");
        }

        // Check reserve ratio
        if !self.check_reserve_ratio(current_day)? {
            bail!("Reserve ratio below minimum threshold");
        }

        // Get current NAV
        let snapshot = self
            .get_nav(current_day)
            .ok_or_else(|| anyhow::anyhow!("NAV not available"))?;

        // Compute expected redemption amount
        let expected_stable = token_amount * snapshot.nav_value / 1_000_000;

        let execution_day = current_day + self.settlement_days;

        let request = RedemptionRequest {
            request_id,
            requester,
            token_amount,
            nav_at_request: snapshot.nav_value,
            expected_stable,
            request_day: current_day,
            execution_day,
            status: RedemptionStatus::Pending,
        };

        // Enqueue
        self.redemption_queue
            .entry(execution_day)
            .or_default()
            .push(request.clone());

        Ok(request)
    }

    /// Process redemption queue for specific day
    pub fn process_redemptions(&mut self, day: u64) -> Result<Vec<RedemptionRequest>> {
        let requests = self.redemption_queue.remove(&day).unwrap_or_default();
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        let mut executed = Vec::new();
        let mut remaining_quota =
            self.daily_quota - self.quota_used.get(&day).copied().unwrap_or(0);

        for mut request in requests {
            if request.status != RedemptionStatus::Pending {
                continue;
            }

            if request.expected_stable <= remaining_quota {
                // Quota sufficient: execute redemption
                request.status = RedemptionStatus::Executed;
                remaining_quota = remaining_quota.saturating_sub(request.expected_stable);
                self.total_redeemed = self.total_redeemed.saturating_add(request.expected_stable);
                executed.push(request);
            } else {
                // Quota insufficient: postpone to next day
                request.execution_day += 1;
                self.redemption_queue
                    .entry(day + 1)
                    .or_default()
                    .push(request);
            }
        }

        // 更新已使用配额
        let used = self.daily_quota - remaining_quota;
        self.quota_used.insert(day, used);

        Ok(executed)
    }

    /// Cancel redemption request
    pub fn cancel_redemption(&mut self, request_id: [u8; 32], day: u64) -> Result<()> {
        let requests = self
            .redemption_queue
            .get_mut(&day)
            .ok_or_else(|| anyhow::anyhow!("No redemptions for day {}", day))?;

        for req in requests.iter_mut() {
            if req.request_id == request_id && req.status == RedemptionStatus::Pending {
                req.status = RedemptionStatus::Cancelled;
                return Ok(());
            }
        }

        bail!("Redemption request not found or already processed")
    }

    /// Get remaining quota
    pub fn remaining_quota(&self, day: u64) -> u128 {
        self.daily_quota - self.quota_used.get(&day).copied().unwrap_or(0)
    }

    /// Get queue length
    pub fn queue_length(&self, day: u64) -> usize {
        self.redemption_queue
            .get(&day)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(id: u8) -> Address {
        Address::from_bytes([id; 32])
    }

    fn req_id(id: u8) -> [u8; 32] {
        [id; 32]
    }

    #[test]
    fn test_nav_snapshot() {
        let mut manager = NavRedemptionManager::new(1_000_000, 5000, 7);

        // Reserves 1M USDT (1e6), treasury 500K, M1 = 100K
        // NAV = (1_000_000 + 500_000) / 100_000 = 15 USDT per Token
        let snapshot = manager
            .record_nav_snapshot(1, 1_000_000, 500_000, 100_000)
            .expect("snapshot");

        assert_eq!(snapshot.nav_value, 15_000_000); // 15 USDT * 1e6 normalized
    }

    #[test]
    fn test_reserve_ratio_check() {
        let mut manager = NavRedemptionManager::new(1_000_000, 5000, 7);

        // Reserve 40%, below 50% threshold
        manager
            .record_nav_snapshot(1, 400_000, 600_000, 100_000)
            .expect("snapshot");

        let result = manager.check_reserve_ratio(1).expect("check");
        assert!(!result);

        // Reserve 60%, meets threshold
        manager
            .record_nav_snapshot(2, 600_000, 400_000, 100_000)
            .expect("snapshot");

        let result = manager.check_reserve_ratio(2).expect("check");
        assert!(result);
    }

    #[test]
    fn test_redemption_submission() {
        let mut manager = NavRedemptionManager::new(1_000_000, 5000, 7);

        // Set NAV = 10 USDT/Token
        manager
            .record_nav_snapshot(1, 800_000, 200_000, 100_000)
            .expect("snapshot");

        // Submit redemption for 1000 tokens
        let request = manager
            .submit_redemption(req_id(1), addr(1), 1_000, 1)
            .expect("submit");

        assert_eq!(request.token_amount, 1_000);
        assert_eq!(request.expected_stable, 10_000); // 1000 * 10
        assert_eq!(request.execution_day, 8); // 1 + 7
        assert_eq!(manager.queue_length(8), 1);
    }

    #[test]
    fn test_redemption_execution() {
        let mut manager = NavRedemptionManager::new(100_000, 5000, 7);

        manager
            .record_nav_snapshot(1, 800_000, 200_000, 100_000)
            .expect("snapshot");

        // Submit multiple redemption requests
        manager
            .submit_redemption(req_id(1), addr(1), 1_000, 1)
            .expect("submit");
        manager
            .submit_redemption(req_id(2), addr(2), 500, 1)
            .expect("submit");

        // Process day 8 redemptions
        let executed = manager.process_redemptions(8).expect("process");

        assert_eq!(executed.len(), 2);
        assert_eq!(executed[0].status, RedemptionStatus::Executed);
        assert_eq!(manager.total_redeemed, 15_000); // 10_000 + 5_000
    }

    #[test]
    fn test_daily_quota_limit() {
        let mut manager = NavRedemptionManager::new(10_000, 5000, 7);

        manager
            .record_nav_snapshot(1, 800_000, 200_000, 100_000)
            .expect("snapshot");

        // Submit redemptions exceeding quota
        manager
            .submit_redemption(req_id(1), addr(1), 1_000, 1)
            .expect("submit"); // 10_000
        manager
            .submit_redemption(req_id(2), addr(2), 500, 1)
            .expect("submit"); // 5_000

        let executed = manager.process_redemptions(8).expect("process");

        // Only first executed, second postponed
        assert_eq!(executed.len(), 1);
        assert_eq!(manager.queue_length(9), 1); // 第二个推迟到 day 9
    }

    #[test]
    fn test_redemption_cancellation() {
        let mut manager = NavRedemptionManager::new(1_000_000, 5000, 7);

        manager
            .record_nav_snapshot(1, 800_000, 200_000, 100_000)
            .expect("snapshot");

        manager
            .submit_redemption(req_id(1), addr(1), 1_000, 1)
            .expect("submit");

        manager.cancel_redemption(req_id(1), 8).expect("cancel");

        let executed = manager.process_redemptions(8).expect("process");
        assert_eq!(executed.len(), 0); // 已取消,不执行
    }

    #[test]
    fn test_reserve_threshold_blocks_redemption() {
        let mut manager = NavRedemptionManager::new(1_000_000, 5000, 7);

        // Reserve ratio only 30%
        manager
            .record_nav_snapshot(1, 300_000, 700_000, 100_000)
            .expect("snapshot");

        let result = manager.submit_redemption(req_id(1), addr(1), 1_000, 1);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Reserve ratio below minimum"));
    }
}
