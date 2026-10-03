// Bond module (clean UTF-8)
//
// SuperVM Treasury Bonds (SVM Bond):
// - Terms: 1y / 3y / 5y
// - Coupon rates: 8% / 12% / 15%
// - Issue price: relative to NAV (min 105%)
// - Redemption: principal + accumulated interest at maturity

use std::collections::HashMap;

use anyhow::{bail, Result};

use crate::types::Address;

// Bond types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BondType {
    OneYear,
    ThreeYear,
    FiveYear,
}

impl BondType {
    // Deprecated: use BondConfig for runtime coupon rates
    #[deprecated(note = "Use BondConfig coupon fields instead")]
    pub fn annual_rate_bp(&self) -> u16 {
        match self {
            BondType::OneYear => 800,
            BondType::ThreeYear => 1_200,
            BondType::FiveYear => 1_500,
        }
    }

    pub fn term_days(&self) -> u64 {
        match self {
            BondType::OneYear => 365,
            BondType::ThreeYear => 365 * 3,
            BondType::FiveYear => 365 * 5,
        }
    }
}

// Hot-reloadable bonds config
#[derive(Debug, Clone)]
pub struct BondConfig {
    pub one_year_coupon_bp: u16,   // default 800 (8%)
    pub three_year_coupon_bp: u16, // default 1200 (12%)
    pub five_year_coupon_bp: u16,  // default 1500 (15%)
    pub min_issue_price_bp: u16,   // default 10500 (105%)
}

impl Default for BondConfig {
    fn default() -> Self {
        Self {
            one_year_coupon_bp: 800,
            three_year_coupon_bp: 1_200,
            five_year_coupon_bp: 1_500,
            min_issue_price_bp: 10_500,
        }
    }
}

// Bond instance
#[derive(Debug, Clone)]
pub struct Bond {
    pub bond_id: [u8; 32],
    pub bond_type: BondType,
    pub holder: Address,
    pub principal: u128,
    pub issue_day: u64,
    pub maturity_day: u64,
    pub issue_price_bp: u16, // relative to NAV, e.g. 10500 = 105%
    pub coupon_rate_bp: u16, // locked at issuance
    pub status: BondStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondStatus {
    Active,
    Matured,
    Redeemed,
}

impl Bond {
    pub fn maturity_value(&self) -> u128 {
        let rate = self.coupon_rate_bp as u128;
        let years = self.bond_type.term_days() / 365;
        let interest = self.principal * rate * years as u128 / 10_000;
        self.principal + interest
    }

    pub fn is_matured(&self, current_day: u64) -> bool {
        current_day >= self.maturity_day
    }
}

// Bond manager
pub struct BondManager {
    bonds: HashMap<[u8; 32], Bond>,
    holdings: HashMap<Address, Vec<[u8; 32]>>,
    stats_by_type: HashMap<BondType, (u64, u128)>,
    total_issued: u128,
    total_redeemed: u128,
    config: BondConfig,
}

impl BondManager {
    pub fn new() -> Self {
        Self::new_with_config(BondConfig::default())
    }

    pub fn new_with_config(config: BondConfig) -> Self {
        Self {
            bonds: HashMap::new(),
            holdings: HashMap::new(),
            stats_by_type: HashMap::new(),
            total_issued: 0,
            total_redeemed: 0,
            config,
        }
    }

    pub fn config(&self) -> &BondConfig {
        &self.config
    }

    pub fn set_one_year_coupon_bp(&mut self, coupon_bp: u16) {
        self.config.one_year_coupon_bp = coupon_bp.min(5_000); // cap 50%
    }

    pub fn set_three_year_coupon_bp(&mut self, coupon_bp: u16) {
        self.config.three_year_coupon_bp = coupon_bp.min(5_000);
    }

    pub fn set_five_year_coupon_bp(&mut self, coupon_bp: u16) {
        self.config.five_year_coupon_bp = coupon_bp.min(5_000);
    }

    pub fn set_min_issue_price_bp(&mut self, price_bp: u16) {
        self.config.min_issue_price_bp = price_bp.clamp(10_000, 20_000); // 100%-200%
    }

    fn get_coupon_rate(&self, bond_type: BondType) -> u16 {
        match bond_type {
            BondType::OneYear => self.config.one_year_coupon_bp,
            BondType::ThreeYear => self.config.three_year_coupon_bp,
            BondType::FiveYear => self.config.five_year_coupon_bp,
        }
    }

    pub fn issue_bond(
        &mut self,
        bond_id: [u8; 32],
        bond_type: BondType,
        holder: Address,
        principal: u128,
        issue_day: u64,
        issue_price_bp: u16,
    ) -> Result<Bond> {
        if self.bonds.contains_key(&bond_id) {
            bail!("Bond ID already exists");
        }
        if principal == 0 {
            bail!("Principal must be > 0");
        }
        if issue_price_bp < self.config.min_issue_price_bp {
            bail!(
                "Issue price must be >= {}% ({} bp)",
                self.config.min_issue_price_bp / 100,
                self.config.min_issue_price_bp
            );
        }

        let maturity_day = issue_day + bond_type.term_days();
        let coupon_rate_bp = self.get_coupon_rate(bond_type);

        let bond = Bond {
            bond_id,
            bond_type,
            holder,
            principal,
            issue_day,
            maturity_day,
            issue_price_bp,
            coupon_rate_bp,
            status: BondStatus::Active,
        };

        self.bonds.insert(bond_id, bond.clone());
        self.holdings.entry(holder).or_default().push(bond_id);

        let (count, total) = self.stats_by_type.entry(bond_type).or_insert((0, 0));
        *count += 1;
        *total = total.saturating_add(principal);
        self.total_issued = self.total_issued.saturating_add(principal);

        Ok(bond)
    }

    pub fn redeem_bond(&mut self, bond_id: [u8; 32], current_day: u64) -> Result<u128> {
        let bond = self
            .bonds
            .get_mut(&bond_id)
            .ok_or_else(|| anyhow::anyhow!("Bond not found"))?;

        if bond.status != BondStatus::Active {
            bail!("Bond is not active");
        }
        if !bond.is_matured(current_day) {
            bail!("Bond has not matured yet");
        }

        let maturity_value = bond.maturity_value();
        bond.status = BondStatus::Redeemed;
        self.total_redeemed = self.total_redeemed.saturating_add(maturity_value);
        Ok(maturity_value)
    }

    pub fn transfer_bond(&mut self, bond_id: [u8; 32], from: Address, to: Address) -> Result<()> {
        let bond = self
            .bonds
            .get_mut(&bond_id)
            .ok_or_else(|| anyhow::anyhow!("Bond not found"))?;

        if bond.holder != from {
            bail!("Not the bond holder");
        }
        if bond.status != BondStatus::Active {
            bail!("Can only transfer active bonds");
        }

        if let Some(bonds) = self.holdings.get_mut(&from) {
            bonds.retain(|id| id != &bond_id);
        }
        self.holdings.entry(to).or_default().push(bond_id);
        bond.holder = to;
        Ok(())
    }

    pub fn get_bond(&self, bond_id: [u8; 32]) -> Option<&Bond> {
        self.bonds.get(&bond_id)
    }

    pub fn get_holdings(&self, holder: Address) -> Vec<&Bond> {
        self.holdings
            .get(&holder)
            .map(|ids| ids.iter().filter_map(|id| self.bonds.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn get_stats(&self, bond_type: BondType) -> (u64, u128) {
        self.stats_by_type
            .get(&bond_type)
            .copied()
            .unwrap_or((0, 0))
    }

    pub fn mark_matured_bonds(&mut self, current_day: u64) -> usize {
        let mut count = 0;
        for bond in self.bonds.values_mut() {
            if bond.status == BondStatus::Active && bond.is_matured(current_day) {
                bond.status = BondStatus::Matured;
                count += 1;
            }
        }
        count
    }

    pub fn total_issued(&self) -> u128 {
        self.total_issued
    }

    pub fn total_redeemed(&self) -> u128 {
        self.total_redeemed
    }

    pub fn outstanding(&self) -> u128 {
        self.total_issued.saturating_sub(self.total_redeemed)
    }
}

impl Default for BondManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(id: u8) -> Address {
        Address::from_bytes([id; 32])
    }
    fn bond_id(id: u8) -> [u8; 32] {
        [id; 32]
    }

    #[test]
    #[allow(deprecated)]
    fn test_bond_types() {
        assert_eq!(BondType::OneYear.annual_rate_bp(), 800);
        assert_eq!(BondType::OneYear.term_days(), 365);
        assert_eq!(BondType::ThreeYear.annual_rate_bp(), 1200);
        assert_eq!(BondType::ThreeYear.term_days(), 365 * 3);
        assert_eq!(BondType::FiveYear.annual_rate_bp(), 1500);
        assert_eq!(BondType::FiveYear.term_days(), 365 * 5);
    }

    #[test]
    fn test_bond_maturity_value() {
        let bond = Bond {
            bond_id: bond_id(1),
            bond_type: BondType::ThreeYear,
            holder: addr(1),
            principal: 10_000,
            issue_day: 1,
            maturity_day: 1 + 365 * 3,
            issue_price_bp: 10_500,
            coupon_rate_bp: 1_200,
            status: BondStatus::Active,
        };
        assert_eq!(bond.maturity_value(), 13_600);
    }

    #[test]
    fn test_issue_bond() {
        let mut manager = BondManager::new();
        let bond = manager
            .issue_bond(bond_id(1), BondType::OneYear, addr(1), 10_000, 1, 10_500)
            .expect("issue");
        assert_eq!(bond.principal, 10_000);
        assert_eq!(bond.maturity_day, 366);
        assert_eq!(manager.total_issued(), 10_000);
        let (count, total) = manager.get_stats(BondType::OneYear);
        assert_eq!(count, 1);
        assert_eq!(total, 10_000);
    }

    #[test]
    fn test_redeem_bond() {
        let mut manager = BondManager::new();
        manager
            .issue_bond(bond_id(1), BondType::OneYear, addr(1), 10_000, 1, 10_500)
            .expect("issue");
        assert!(manager.redeem_bond(bond_id(1), 365).is_err());
        let value = manager.redeem_bond(bond_id(1), 366).expect("redeem");
        assert_eq!(value, 10_800);
        let bond = manager.get_bond(bond_id(1)).expect("bond");
        assert_eq!(bond.status, BondStatus::Redeemed);
        assert_eq!(manager.total_redeemed(), 10_800);
        assert_eq!(manager.outstanding(), 0);
    }

    #[test]
    fn test_transfer_bond() {
        let mut manager = BondManager::new();
        manager
            .issue_bond(bond_id(1), BondType::ThreeYear, addr(1), 5_000, 1, 10_500)
            .expect("issue");
        manager
            .transfer_bond(bond_id(1), addr(1), addr(2))
            .expect("transfer");
        let bond = manager.get_bond(bond_id(1)).expect("bond");
        assert_eq!(bond.holder, addr(2));
        assert_eq!(manager.get_holdings(addr(1)).len(), 0);
        assert_eq!(manager.get_holdings(addr(2)).len(), 1);
    }

    #[test]
    fn test_mark_matured_bonds() {
        let mut manager = BondManager::new();
        manager
            .issue_bond(bond_id(1), BondType::OneYear, addr(1), 10_000, 1, 10_500)
            .expect("issue");
        manager
            .issue_bond(bond_id(2), BondType::ThreeYear, addr(2), 20_000, 1, 10_500)
            .expect("issue");
        let count = manager.mark_matured_bonds(366);
        assert_eq!(count, 1);
        assert_eq!(
            manager.get_bond(bond_id(1)).unwrap().status,
            BondStatus::Matured
        );
        assert_eq!(
            manager.get_bond(bond_id(2)).unwrap().status,
            BondStatus::Active
        );
    }

    #[test]
    fn test_multiple_holdings() {
        let mut manager = BondManager::new();
        manager
            .issue_bond(bond_id(1), BondType::OneYear, addr(1), 10_000, 1, 10_500)
            .expect("issue");
        manager
            .issue_bond(bond_id(2), BondType::ThreeYear, addr(1), 20_000, 1, 10_500)
            .expect("issue");
        let holdings = manager.get_holdings(addr(1));
        assert_eq!(holdings.len(), 2);
        let total_principal: u128 = holdings.iter().map(|b| b.principal).sum();
        assert_eq!(total_principal, 30_000);
    }

    #[test]
    fn test_invalid_issue_price() {
        let mut manager = BondManager::new();
        let result = manager.issue_bond(bond_id(1), BondType::OneYear, addr(1), 10_000, 1, 9_500);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Issue price must be >= 105%"));
    }
}
