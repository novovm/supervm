//! CDP module (collateralized stablecoin)
//!
//! Multi-tier collateral minting for M2 stablecoin:
//! - Tier 1: Mainnet Token (150% collateral ratio)
//! - Tier 2: SVM Bond (120% collateral ratio)
//! - Tier 3: BTC/ETH (200% collateral ratio)
//! - Tier 4: RWA (250% collateral ratio)
//!
//! Liquidation: triggered when collateral ratio falls below threshold

use crate::types::Address;
use anyhow::{bail, Result};
use std::collections::HashMap;

/// Collateral types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollateralType {
    MainnetToken, // Tier 1: Mainnet Token
    SvmBond,      // Tier 2: SVM bond
    BtcEth,       // Tier 3: BTC/ETH
    Rwa,          // Tier 4: RWA assets
}

impl CollateralType {
    /// Get collateral ratio (bps, 15000 = 150%) - deprecated, use CdpConfig
    #[deprecated(note = "Use CdpConfig::min_collateral_ratio_bp instead")]
    pub fn collateral_ratio_bp(&self) -> u16 {
        match self {
            CollateralType::MainnetToken => 15_000,
            CollateralType::SvmBond => 12_000,
            CollateralType::BtcEth => 20_000,
            CollateralType::Rwa => 25_000,
        }
    }

    /// Liquidation threshold (bps, 12500 = 125%) - deprecated, use CdpConfig
    #[deprecated(note = "Use CdpConfig::liquidation_threshold_bp instead")]
    pub fn liquidation_threshold_bp(&self) -> u16 {
        match self {
            CollateralType::MainnetToken => 12_500,
            CollateralType::SvmBond => 11_000,
            CollateralType::BtcEth => 17_500,
            CollateralType::Rwa => 22_000,
        }
    }
}

/// CDP configuration (hot-reloadable)
#[derive(Debug, Clone)]
pub struct CdpConfig {
    /// Minimum collateral ratio (bps, default 15000 = 150%)
    pub min_collateral_ratio_bp: u16,
    /// Liquidation threshold (bps, default 12500 = 125%)
    pub liquidation_threshold_bp: u16,
    /// Stability fee (annual bps, default 200 = 2%)
    pub stability_fee_bp: u16,
    /// Liquidation penalty (bps, default 1000 = 10%)
    pub liquidation_penalty_bp: u16,
}

impl Default for CdpConfig {
    fn default() -> Self {
        Self {
            min_collateral_ratio_bp: 15_000,
            liquidation_threshold_bp: 12_500,
            stability_fee_bp: 200,
            liquidation_penalty_bp: 1_000,
        }
    }
}
/// CDP position
#[derive(Debug, Clone)]
pub struct CdpPosition {
    /// CDP ID
    pub cdp_id: [u8; 32],
    /// Owner
    pub owner: Address,
    /// Collateral type
    pub collateral_type: CollateralType,
    /// Collateral amount (token smallest unit)
    pub collateral_amount: u128,
    /// Collateral value (stablecoin smallest unit)
    pub collateral_value: u128,
    /// Debt (minted stablecoin amount)
    pub debt: u128,
    /// Created time (Unix days)
    pub created_day: u64,
    /// Status
    pub status: CdpStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdpStatus {
    Active,      // normal
    AtRisk,      // near liquidation threshold
    Liquidating, // in liquidation
    Closed,      // closed
}

impl CdpPosition {
    /// Compute current collateral ratio (bps)
    pub fn current_ratio_bp(&self) -> u16 {
        if self.debt == 0 {
            return u16::MAX;
        }
        let ratio = self
            .collateral_value
            .saturating_mul(10_000)
            .checked_div(self.debt)
            .unwrap_or(u128::MAX);
        u16::try_from(ratio).unwrap_or(u16::MAX)
    }

    /// Check if liquidation needed (using configured threshold)
    pub fn should_liquidate(&self, liquidation_threshold_bp: u16) -> bool {
        let current = self.current_ratio_bp();
        current < liquidation_threshold_bp
    }

    /// Compute max mintable stablecoin amount
    pub fn max_mintable(&self, collateral_ratio_bp: u16) -> u128 {
        (self.collateral_value * 10_000 / collateral_ratio_bp as u128).saturating_sub(self.debt)
    }
}

/// CDP manager
pub struct CdpManager {
    /// All CDPs (cdp_id -> CdpPosition)
    cdps: HashMap<[u8; 32], CdpPosition>,
    /// User CDPs (owner -> cdp_ids)
    user_cdps: HashMap<Address, Vec<[u8; 32]>>,
    /// Stats by collateral type (type -> count, total_collateral, total_debt)
    stats: HashMap<CollateralType, (u64, u128, u128)>,
    /// Total stablecoin supply
    total_supply: u128,
    /// CDP configuration (hot-reloadable)
    config: CdpConfig,
}

impl CdpManager {
    /// Construct manager with explicit config
    pub fn new_with_config(config: CdpConfig) -> Self {
        Self {
            cdps: HashMap::new(),
            user_cdps: HashMap::new(),
            stats: HashMap::new(),
            total_supply: 0,
            config,
        }
    }

    /// Construct manager with default config (backward compatible)
    pub fn new(liquidation_penalty_bp: u16) -> Self {
        let config = CdpConfig {
            liquidation_penalty_bp,
            ..Default::default()
        };
        Self::new_with_config(config)
    }

    /// Set liquidation penalty (bps)
    pub fn set_liquidation_penalty_bp(&mut self, penalty_bp: u16) {
        let capped = penalty_bp.min(5_000);
        self.config.liquidation_penalty_bp = capped;
    }

    /// Get liquidation penalty (bps)
    pub fn liquidation_penalty_bp(&self) -> u16 {
        self.config.liquidation_penalty_bp
    }

    /// Set minimum collateral ratio (bps)
    pub fn set_min_collateral_ratio_bp(&mut self, ratio_bp: u16) {
        let capped = ratio_bp.clamp(10_000, 50_000); // 100%-500%
        self.config.min_collateral_ratio_bp = capped;
    }

    /// Set liquidation threshold (bps)
    pub fn set_liquidation_threshold_bp(&mut self, threshold_bp: u16) {
        let capped = threshold_bp.clamp(10_000, 50_000);
        self.config.liquidation_threshold_bp = capped;
    }

    /// Set stability fee (annual bps)
    pub fn set_stability_fee_bp(&mut self, fee_bp: u16) {
        let capped = fee_bp.min(10_000); // max 100%
        self.config.stability_fee_bp = capped;
    }

    /// Get config reference
    pub fn config(&self) -> &CdpConfig {
        &self.config
    }

    /// Open a CDP
    #[allow(clippy::too_many_arguments)]
    pub fn open_cdp(
        &mut self,
        cdp_id: [u8; 32],
        owner: Address,
        collateral_type: CollateralType,
        collateral_amount: u128,
        collateral_value: u128,
        mint_amount: u128,
        current_day: u64,
    ) -> Result<CdpPosition> {
        if self.cdps.contains_key(&cdp_id) {
            bail!("CDP ID already exists");
        }

        if collateral_amount == 0 || collateral_value == 0 {
            bail!("Collateral must be > 0");
        }

        if mint_amount == 0 {
            bail!("Mint amount must be > 0");
        }

        // Check collateral ratio
        let ratio_bp = (collateral_value * 10_000 / mint_amount) as u16;
        if ratio_bp < self.config.min_collateral_ratio_bp {
            bail!("Insufficient collateral ratio");
        }

        let cdp = CdpPosition {
            cdp_id,
            owner,
            collateral_type,
            collateral_amount,
            collateral_value,
            debt: mint_amount,
            created_day: current_day,
            status: CdpStatus::Active,
        };

        self.cdps.insert(cdp_id, cdp.clone());
        self.user_cdps.entry(owner).or_default().push(cdp_id);

        // Update stats
        let (count, coll, debt) = self.stats.entry(collateral_type).or_insert((0, 0, 0));
        *count += 1;
        *coll = coll.saturating_add(collateral_value);
        *debt = debt.saturating_add(mint_amount);
        self.total_supply = self.total_supply.saturating_add(mint_amount);

        Ok(cdp)
    }

    /// Add collateral
    pub fn add_collateral(&mut self, cdp_id: [u8; 32], amount: u128, value: u128) -> Result<()> {
        let cdp = self
            .cdps
            .get_mut(&cdp_id)
            .ok_or_else(|| anyhow::anyhow!("CDP not found"))?;

        if cdp.status != CdpStatus::Active && cdp.status != CdpStatus::AtRisk {
            bail!("CDP is not active");
        }

        cdp.collateral_amount = cdp.collateral_amount.saturating_add(amount);
        cdp.collateral_value = cdp.collateral_value.saturating_add(value);

        // Update status
        if cdp.current_ratio_bp() >= self.config.min_collateral_ratio_bp {
            cdp.status = CdpStatus::Active;
        }

        // Update stats
        if let Some((_, coll, _)) = self.stats.get_mut(&cdp.collateral_type) {
            *coll = coll.saturating_add(value);
        }

        Ok(())
    }

    /// Mint more stablecoin
    pub fn mint_more(&mut self, cdp_id: [u8; 32], amount: u128) -> Result<()> {
        let cdp = self
            .cdps
            .get_mut(&cdp_id)
            .ok_or_else(|| anyhow::anyhow!("CDP not found"))?;

        if cdp.status != CdpStatus::Active {
            bail!("CDP is not active");
        }

        let new_debt = cdp.debt.saturating_add(amount);
        let ratio_bp = (cdp.collateral_value * 10_000 / new_debt) as u16;

        if ratio_bp < self.config.min_collateral_ratio_bp {
            bail!("Would exceed collateral ratio");
        }

        cdp.debt = new_debt;
        self.total_supply = self.total_supply.saturating_add(amount);

        // Update stats
        if let Some((_, _, debt)) = self.stats.get_mut(&cdp.collateral_type) {
            *debt = debt.saturating_add(amount);
        }

        Ok(())
    }

    /// Repay debt
    pub fn repay(&mut self, cdp_id: [u8; 32], amount: u128) -> Result<()> {
        let cdp = self
            .cdps
            .get_mut(&cdp_id)
            .ok_or_else(|| anyhow::anyhow!("CDP not found"))?;

        if cdp.status == CdpStatus::Closed {
            bail!("CDP is closed");
        }

        let repay_amount = amount.min(cdp.debt);
        cdp.debt = cdp.debt.saturating_sub(repay_amount);
        self.total_supply = self.total_supply.saturating_sub(repay_amount);

        // Update stats
        if let Some((_, _, debt)) = self.stats.get_mut(&cdp.collateral_type) {
            *debt = debt.saturating_sub(repay_amount);
        }

        // Close if fully repaid
        if cdp.debt == 0 {
            cdp.status = CdpStatus::Closed;
        } else if cdp.status == CdpStatus::AtRisk
            && cdp.current_ratio_bp() >= self.config.min_collateral_ratio_bp
        {
            cdp.status = CdpStatus::Active;
        }

        Ok(())
    }

    /// Update collateral price
    pub fn update_collateral_price(&mut self, cdp_id: [u8; 32], new_value: u128) -> Result<()> {
        let cdp = self
            .cdps
            .get_mut(&cdp_id)
            .ok_or_else(|| anyhow::anyhow!("CDP not found"))?;

        let old_value = cdp.collateral_value;
        cdp.collateral_value = new_value;

        // Update stats
        if let Some((_, coll, _)) = self.stats.get_mut(&cdp.collateral_type) {
            *coll = coll.saturating_sub(old_value).saturating_add(new_value);
        }

        // Mark as at-risk if below threshold
        if cdp.should_liquidate(self.config.liquidation_threshold_bp) {
            cdp.status = CdpStatus::AtRisk;
        }

        Ok(())
    }

    /// Liquidate CDP
    pub fn liquidate_cdp(&mut self, cdp_id: [u8; 32]) -> Result<(u128, u128)> {
        let cdp = self
            .cdps
            .get_mut(&cdp_id)
            .ok_or_else(|| anyhow::anyhow!("CDP not found"))?;

        if !cdp.should_liquidate(self.config.liquidation_threshold_bp) {
            bail!("CDP does not meet liquidation criteria");
        }

        // Compute liquidation penalty
        let penalty = cdp.debt * self.config.liquidation_penalty_bp as u128 / 10_000;
        let _total_debt = cdp.debt.saturating_add(penalty);

        // Auction collateral
        let collateral_seized = cdp.collateral_amount;
        let collateral_value = cdp.collateral_value;

        // Update stats
        if let Some((count, coll, debt)) = self.stats.get_mut(&cdp.collateral_type) {
            *count = count.saturating_sub(1);
            *coll = coll.saturating_sub(collateral_value);
            *debt = debt.saturating_sub(cdp.debt);
        }

        self.total_supply = self.total_supply.saturating_sub(cdp.debt);

        cdp.status = CdpStatus::Closed;
        cdp.debt = 0;
        cdp.collateral_amount = 0;
        cdp.collateral_value = 0;

        Ok((collateral_seized, penalty))
    }

    /// Get CDP
    pub fn get_cdp(&self, cdp_id: [u8; 32]) -> Option<&CdpPosition> {
        self.cdps.get(&cdp_id)
    }

    /// Get user's CDPs
    pub fn get_user_cdps(&self, owner: Address) -> Vec<&CdpPosition> {
        self.user_cdps
            .get(&owner)
            .map(|ids| ids.iter().filter_map(|id| self.cdps.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get stats
    pub fn get_stats(&self, collateral_type: CollateralType) -> (u64, u128, u128) {
        self.stats
            .get(&collateral_type)
            .copied()
            .unwrap_or((0, 0, 0))
    }

    /// Get total stablecoin supply
    pub fn total_supply(&self) -> u128 {
        self.total_supply
    }

    /// Find CDPs that need liquidation
    pub fn find_liquidatable_cdps(&self) -> Vec<[u8; 32]> {
        let threshold = self.config.liquidation_threshold_bp;
        self.cdps
            .iter()
            .filter(|(_, cdp)| cdp.should_liquidate(threshold))
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(id: u8) -> Address {
        Address::from_bytes([id; 32])
    }

    fn cdp_id(id: u8) -> [u8; 32] {
        [id; 32]
    }

    #[test]
    #[allow(deprecated)]
    fn test_collateral_types() {
        assert_eq!(CollateralType::MainnetToken.collateral_ratio_bp(), 15_000);
        assert_eq!(
            CollateralType::MainnetToken.liquidation_threshold_bp(),
            12_500
        );

        assert_eq!(CollateralType::SvmBond.collateral_ratio_bp(), 12_000);
        assert_eq!(CollateralType::BtcEth.collateral_ratio_bp(), 20_000);
        assert_eq!(CollateralType::Rwa.collateral_ratio_bp(), 25_000);
    }

    #[test]
    fn test_open_cdp() {
        let mut manager = CdpManager::new(500);

        // 抵押 1000 Token (价值 15000 USDT), 铸造 10000 稳定币
        // 抵押率 = 15000 / 10000 = 150%
        let cdp = manager
            .open_cdp(
                cdp_id(1),
                addr(1),
                CollateralType::MainnetToken,
                1_000,
                15_000,
                10_000,
                1,
            )
            .expect("open");

        assert_eq!(cdp.collateral_amount, 1_000);
        assert_eq!(cdp.debt, 10_000);
        assert_eq!(cdp.current_ratio_bp(), 15_000);
        assert_eq!(manager.total_supply(), 10_000);
    }

    #[test]
    fn test_add_collateral() {
        let mut manager = CdpManager::new(500);

        manager
            .open_cdp(
                cdp_id(1),
                addr(1),
                CollateralType::MainnetToken,
                1_000,
                15_000,
                10_000,
                1,
            )
            .expect("open");

        manager.add_collateral(cdp_id(1), 500, 7_500).expect("add");

        let cdp = manager.get_cdp(cdp_id(1)).expect("cdp");
        assert_eq!(cdp.collateral_value, 22_500);
        assert_eq!(cdp.current_ratio_bp(), 22_500); // 22500 / 10000 * 100 = 225%
    }

    #[test]
    fn test_mint_more() {
        let mut manager = CdpManager::new(500);

        manager
            .open_cdp(
                cdp_id(1),
                addr(1),
                CollateralType::MainnetToken,
                1_000,
                15_000,
                5_000,
                1,
            )
            .expect("open");

        // 可以再铸造 5000 (15000 / 1.5 = 10000 max)
        manager.mint_more(cdp_id(1), 5_000).expect("mint");

        let cdp = manager.get_cdp(cdp_id(1)).expect("cdp");
        assert_eq!(cdp.debt, 10_000);
        assert_eq!(manager.total_supply(), 10_000);
    }

    #[test]
    fn test_repay() {
        let mut manager = CdpManager::new(500);

        manager
            .open_cdp(
                cdp_id(1),
                addr(1),
                CollateralType::MainnetToken,
                1_000,
                15_000,
                10_000,
                1,
            )
            .expect("open");

        manager.repay(cdp_id(1), 5_000).expect("repay");

        let cdp = manager.get_cdp(cdp_id(1)).expect("cdp");
        assert_eq!(cdp.debt, 5_000);
        assert_eq!(manager.total_supply(), 5_000);

        // 全部还清
        manager.repay(cdp_id(1), 5_000).expect("repay");

        let cdp = manager.get_cdp(cdp_id(1)).expect("cdp");
        assert_eq!(cdp.debt, 0);
        assert_eq!(cdp.status, CdpStatus::Closed);
    }

    #[test]
    fn test_liquidation() {
        let mut manager = CdpManager::new(500);

        manager
            .open_cdp(
                cdp_id(1),
                addr(1),
                CollateralType::MainnetToken,
                1_000,
                15_000,
                10_000,
                1,
            )
            .expect("open");

        // 价格下跌到 12000 (抵押率 120% < 125% 清算线)
        manager
            .update_collateral_price(cdp_id(1), 12_000)
            .expect("update");

        let cdp = manager.get_cdp(cdp_id(1)).expect("cdp");
        assert!(cdp.should_liquidate(12_500)); // 使用默认清算阈值 125%

        let (seized, penalty) = manager.liquidate_cdp(cdp_id(1)).expect("liquidate");

        assert_eq!(seized, 1_000);
        assert_eq!(penalty, 500); // 10000 * 5%
        assert_eq!(manager.total_supply(), 0);
    }

    #[test]
    fn test_insufficient_collateral() {
        let mut manager = CdpManager::new(500);

        // 抵押率 100% < 150% 要求
        let result = manager.open_cdp(
            cdp_id(1),
            addr(1),
            CollateralType::MainnetToken,
            1_000,
            10_000,
            10_000,
            1,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Insufficient collateral"));
    }

    #[test]
    fn test_find_liquidatable_cdps() {
        let mut manager = CdpManager::new(500);

        manager
            .open_cdp(
                cdp_id(1),
                addr(1),
                CollateralType::MainnetToken,
                1_000,
                15_000,
                10_000,
                1,
            )
            .expect("open");

        manager
            .open_cdp(
                cdp_id(2),
                addr(2),
                CollateralType::MainnetToken,
                1_000,
                15_000,
                10_000,
                1,
            )
            .expect("open");

        // 第一个 CDP 价格下跌
        manager
            .update_collateral_price(cdp_id(1), 12_000)
            .expect("update");

        let liquidatable = manager.find_liquidatable_cdps();
        assert_eq!(liquidatable.len(), 1);
        assert_eq!(liquidatable[0], cdp_id(1));
    }

    #[test]
    fn test_multi_tier_collateral() {
        let mut manager = CdpManager::new(500);

        // Tier 1: MainnetToken (150%)
        manager
            .open_cdp(
                cdp_id(1),
                addr(1),
                CollateralType::MainnetToken,
                1_000,
                15_000,
                10_000,
                1,
            )
            .expect("open");

        // Tier 2: SvmBond (180% - 符合统一 150% 最低要求)
        manager
            .open_cdp(
                cdp_id(2),
                addr(2),
                CollateralType::SvmBond,
                1_000,
                18_000, // 提高到 180%
                10_000,
                1,
            )
            .expect("open");

        let (count1, coll1, debt1) = manager.get_stats(CollateralType::MainnetToken);
        assert_eq!(count1, 1);
        assert_eq!(coll1, 15_000);
        assert_eq!(debt1, 10_000);

        let (count2, coll2, debt2) = manager.get_stats(CollateralType::SvmBond);
        assert_eq!(count2, 1);
        assert_eq!(coll2, 18_000); // 更新统计验证
        assert_eq!(debt2, 10_000);

        assert_eq!(manager.total_supply(), 20_000);
    }
}

#[cfg(test)]
mod tests_penalty_update {
    use super::*;

    fn addr(b: u8) -> Address {
        Address::from_bytes([b; 32])
    }
    fn cdp_id(b: u8) -> [u8; 32] {
        [b; 32]
    }

    #[test]
    fn liquidation_penalty_is_applied_after_update() {
        let mut mgr = CdpManager::new(1_000); // 10%
                                              // 开仓: 抵押价值 20_000, 债务 10_000 => 抵押率 200%
        let owner = addr(1);
        let _pos = mgr
            .open_cdp(
                cdp_id(1),
                owner,
                CollateralType::MainnetToken,
                2_000,  // collateral amount (units)
                20_000, // collateral value
                10_000, // debt
                0,
            )
            .expect("open");

        // 下跌触发清算
        mgr.update_collateral_price(cdp_id(1), 12_000)
            .expect("update"); // 120%
        assert!(mgr.get_cdp(cdp_id(1)).unwrap().should_liquidate(12_500)); // 120% < 125%

        // 先以 10% 罚金清算
        let (_seized, penalty10) = mgr.liquidate_cdp(cdp_id(1)).expect("liq10");
        assert_eq!(penalty10, 10_000 * 1_000 / 10_000); // 1000

        // 重新开一个仓位用于对比(因为上一个已关闭)
        let _ = mgr
            .open_cdp(
                cdp_id(2),
                owner,
                CollateralType::MainnetToken,
                2_000,
                20_000,
                10_000,
                0,
            )
            .expect("open2");
        mgr.update_collateral_price(cdp_id(2), 12_000)
            .expect("update2");

        // 更新罚金到 15%
        mgr.set_liquidation_penalty_bp(1_500);
        assert_eq!(mgr.liquidation_penalty_bp(), 1_500);

        let (_seized2, penalty15) = mgr.liquidate_cdp(cdp_id(2)).expect("liq15");
        assert_eq!(penalty15, 10_000 * 1_500 / 10_000); // 1500
    }
}
