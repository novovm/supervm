use crate::types::{BFTError, BFTResult, MarketGovernancePolicy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use web30_core::amm::AMMManager;
use web30_core::bonds::BondManager;
use web30_core::cdp::CdpManager;
use web30_core::nav_redemption::NavRedemptionManager;
use web30_core::treasury::{Treasury, TreasuryAccountKind, TreasuryEvent};
use web30_core::treasury_impl::{NavConfig, TreasuryConfig, TreasuryImpl};
use web30_core::types::Address as Web30Address;

const ADDR_DOMAIN_SYSTEM: u8 = 0xC1;
const SYS_ADDR_TREASURY_CONTROLLER: u8 = 0xE0;
const SYS_ADDR_TREASURY_INGRESS: u8 = 0xE1;

fn system_address(tag: u8) -> Web30Address {
    let mut bytes = [0u8; 32];
    bytes[0] = tag;
    bytes[31] = ADDR_DOMAIN_SYSTEM;
    Web30Address::from_bytes(bytes)
}

fn from_web30_error(ctx: &str, err: impl std::fmt::Display) -> BFTError {
    BFTError::InvalidProposal(format!("{}: {}", ctx, err))
}

fn to_u64(value: u128, ctx: &str) -> BFTResult<u64> {
    u64::try_from(value).map_err(|_| BFTError::Internal(format!("{} out of u64 range", ctx)))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Web30MarketEngineSnapshot {
    pub amm_swap_fee_bp: u16,
    pub amm_lp_fee_share_bp: u16,
    pub cdp_min_collateral_ratio_bp: u16,
    pub cdp_liquidation_threshold_bp: u16,
    pub cdp_liquidation_penalty_bp: u16,
    pub cdp_stability_fee_bp: u16,
    pub cdp_max_leverage_x100: u16,
    pub bond_one_year_coupon_bp: u16,
    pub bond_three_year_coupon_bp: u16,
    pub bond_five_year_coupon_bp: u16,
    pub bond_max_maturity_days_policy: u16,
    pub bond_min_issue_price_bp: u16,
    pub reserve_min_reserve_ratio_bp: u16,
    pub reserve_redemption_fee_bp: u16,
    pub nav_settlement_delay_epochs: u16,
    pub nav_max_daily_redemption_bp: u16,
    pub buyback_trigger_discount_bp: u16,
    pub buyback_max_treasury_budget_per_epoch: u64,
    pub buyback_burn_share_bp: u16,
    pub treasury_main_balance: u64,
    pub treasury_ecosystem_balance: u64,
    pub treasury_risk_reserve_balance: u64,
    pub reserve_foreign_usdt_balance: u64,
    pub nav_soft_floor_value: u64,
    pub buyback_last_spent_stable: u64,
    pub buyback_last_burned_token: u64,
}

pub struct Web30MarketEngine {
    #[allow(dead_code)]
    amm: AMMManager,
    #[allow(dead_code)]
    cdp: CdpManager,
    #[allow(dead_code)]
    bond: BondManager,
    #[allow(dead_code)]
    nav: NavRedemptionManager,
    treasury: TreasuryImpl,
    #[allow(dead_code)]
    treasury_controller: Web30Address,
    treasury_ingress: Web30Address,
    snapshot: Web30MarketEngineSnapshot,
}

impl Web30MarketEngine {
    fn build_treasury(
        policy: &MarketGovernancePolicy,
        controller: Web30Address,
        balances: Option<(u128, u128, u128)>,
    ) -> TreasuryImpl {
        let (main_balance, ecosystem_balance, risk_balance) = balances.unwrap_or_else(|| {
            let seed = u128::from(policy.buyback.max_treasury_budget_per_epoch);
            let main = seed.saturating_mul(2);
            let ecosystem = seed / 2;
            let risk = seed / 2;
            (main, ecosystem, risk)
        });
        let mut initial_balances = HashMap::new();
        initial_balances.insert(TreasuryAccountKind::Main, main_balance);
        initial_balances.insert(TreasuryAccountKind::Ecosystem, ecosystem_balance);
        initial_balances.insert(TreasuryAccountKind::RiskReserve, risk_balance);
        TreasuryImpl::new(TreasuryConfig {
            initial_balances,
            controller,
            nav_config: NavConfig {
                min_nav_multiplier_bp: policy.reserve.min_reserve_ratio_bp,
                reserve_valuation_source: "market_engine_policy".to_string(),
            },
        })
    }

    pub fn from_policy(policy: &MarketGovernancePolicy) -> BFTResult<Self> {
        policy.validate()?;
        Self::validate_runtime_bounds(policy)?;
        let treasury_controller = system_address(SYS_ADDR_TREASURY_CONTROLLER);
        let treasury_ingress = system_address(SYS_ADDR_TREASURY_INGRESS);

        let mut runtime = Self {
            amm: AMMManager::new(),
            cdp: CdpManager::new(policy.cdp.liquidation_penalty_bp),
            bond: BondManager::new(),
            nav: NavRedemptionManager::new(
                u128::from(policy.nav.max_daily_redemption_bp),
                policy.reserve.min_reserve_ratio_bp,
                u64::from(policy.nav.settlement_delay_epochs),
            ),
            treasury: Self::build_treasury(policy, treasury_controller, None),
            treasury_controller,
            treasury_ingress,
            snapshot: Web30MarketEngineSnapshot {
                amm_swap_fee_bp: 0,
                amm_lp_fee_share_bp: 0,
                cdp_min_collateral_ratio_bp: 0,
                cdp_liquidation_threshold_bp: 0,
                cdp_liquidation_penalty_bp: 0,
                cdp_stability_fee_bp: 0,
                cdp_max_leverage_x100: 0,
                bond_one_year_coupon_bp: 0,
                bond_three_year_coupon_bp: 0,
                bond_five_year_coupon_bp: 0,
                bond_max_maturity_days_policy: 0,
                bond_min_issue_price_bp: 0,
                reserve_min_reserve_ratio_bp: 0,
                reserve_redemption_fee_bp: 0,
                nav_settlement_delay_epochs: 0,
                nav_max_daily_redemption_bp: 0,
                buyback_trigger_discount_bp: 0,
                buyback_max_treasury_budget_per_epoch: 0,
                buyback_burn_share_bp: 0,
                treasury_main_balance: 0,
                treasury_ecosystem_balance: 0,
                treasury_risk_reserve_balance: 0,
                reserve_foreign_usdt_balance: 0,
                nav_soft_floor_value: 0,
                buyback_last_spent_stable: 0,
                buyback_last_burned_token: 0,
            },
        };
        runtime.reconfigure(policy)?;
        Ok(runtime)
    }

    fn validate_runtime_bounds(policy: &MarketGovernancePolicy) -> BFTResult<()> {
        // Keep governance policy and reused SVM2026 runtime in strict sync.
        // Reject values that would be silently clamped by web30-core managers.
        if policy.cdp.liquidation_penalty_bp > 5_000 {
            return Err(BFTError::InvalidProposal(
                "market engine unsupported: cdp.liquidation_penalty_bp must be <= 5000".to_string(),
            ));
        }
        if policy.bond.coupon_rate_bp > 5_000 {
            return Err(BFTError::InvalidProposal(
                "market engine unsupported: bond.coupon_rate_bp must be <= 5000".to_string(),
            ));
        }
        Ok(())
    }

    pub fn reconfigure(&mut self, policy: &MarketGovernancePolicy) -> BFTResult<()> {
        policy.validate()?;
        Self::validate_runtime_bounds(policy)?;

        self.amm.set_global_fee_bps(policy.amm.swap_fee_bp);

        self.cdp
            .set_min_collateral_ratio_bp(policy.cdp.min_collateral_ratio_bp);
        self.cdp
            .set_liquidation_threshold_bp(policy.cdp.liquidation_threshold_bp);
        self.cdp
            .set_liquidation_penalty_bp(policy.cdp.liquidation_penalty_bp);
        self.cdp.set_stability_fee_bp(policy.cdp.stability_fee_bp);

        self.bond.set_one_year_coupon_bp(policy.bond.coupon_rate_bp);
        self.bond
            .set_three_year_coupon_bp(policy.bond.coupon_rate_bp);
        self.bond
            .set_five_year_coupon_bp(policy.bond.coupon_rate_bp);
        self.bond
            .set_min_issue_price_bp(policy.bond.min_issue_price_bp);

        // nav_redemption config in SVM2026 is constructor-based.
        // Recreate manager to apply governance updates deterministically.
        self.nav = NavRedemptionManager::new(
            u128::from(policy.nav.max_daily_redemption_bp),
            policy.reserve.min_reserve_ratio_bp,
            u64::from(policy.nav.settlement_delay_epochs),
        );
        let persisted_balances = (
            self.treasury.balance_of(TreasuryAccountKind::Main),
            self.treasury.balance_of(TreasuryAccountKind::Ecosystem),
            self.treasury.balance_of(TreasuryAccountKind::RiskReserve),
        );
        self.treasury =
            Self::build_treasury(policy, self.treasury_controller, Some(persisted_balances));
        self.treasury
            .on_income(
                &self.treasury_ingress,
                u128::from(policy.reserve.redemption_fee_bp),
                TreasuryAccountKind::RiskReserve,
            )
            .map_err(|e| from_web30_error("market engine reserve income route", e))?;
        self.treasury
            .collect_foreign_currency(
                "USDT".to_string(),
                u128::from(policy.reserve.min_reserve_ratio_bp),
            )
            .map_err(|e| from_web30_error("market engine reserve foreign collection", e))?;
        let nav_soft_floor_value = self.treasury.calculate_nav();
        let buyback_event = self
            .treasury
            .execute_buyback_and_burn(u128::from(policy.buyback.max_treasury_budget_per_epoch))
            .map_err(|e| from_web30_error("market engine buyback execution", e))?;
        let (buyback_last_spent_stable, buyback_last_burned_token) = match buyback_event {
            TreasuryEvent::BuybackAndBurn {
                spent_stable,
                burned_token,
            } => (spent_stable, burned_token),
            _ => {
                return Err(BFTError::Internal(
                    "market engine buyback returned unexpected treasury event".to_string(),
                ))
            }
        };

        let cdp_cfg = self.cdp.config();
        let bond_cfg = self.bond.config();
        self.snapshot = Web30MarketEngineSnapshot {
            amm_swap_fee_bp: policy.amm.swap_fee_bp,
            amm_lp_fee_share_bp: policy.amm.lp_fee_share_bp,
            cdp_min_collateral_ratio_bp: cdp_cfg.min_collateral_ratio_bp,
            cdp_liquidation_threshold_bp: cdp_cfg.liquidation_threshold_bp,
            cdp_liquidation_penalty_bp: cdp_cfg.liquidation_penalty_bp,
            cdp_stability_fee_bp: cdp_cfg.stability_fee_bp,
            cdp_max_leverage_x100: policy.cdp.max_leverage_x100,
            bond_one_year_coupon_bp: bond_cfg.one_year_coupon_bp,
            bond_three_year_coupon_bp: bond_cfg.three_year_coupon_bp,
            bond_five_year_coupon_bp: bond_cfg.five_year_coupon_bp,
            bond_max_maturity_days_policy: policy.bond.max_maturity_days,
            bond_min_issue_price_bp: bond_cfg.min_issue_price_bp,
            reserve_min_reserve_ratio_bp: policy.reserve.min_reserve_ratio_bp,
            reserve_redemption_fee_bp: policy.reserve.redemption_fee_bp,
            nav_settlement_delay_epochs: policy.nav.settlement_delay_epochs,
            nav_max_daily_redemption_bp: policy.nav.max_daily_redemption_bp,
            buyback_trigger_discount_bp: policy.buyback.trigger_discount_bp,
            buyback_max_treasury_budget_per_epoch: policy.buyback.max_treasury_budget_per_epoch,
            buyback_burn_share_bp: policy.buyback.burn_share_bp,
            treasury_main_balance: to_u64(
                self.treasury.balance_of(TreasuryAccountKind::Main),
                "treasury_main_balance",
            )?,
            treasury_ecosystem_balance: to_u64(
                self.treasury.balance_of(TreasuryAccountKind::Ecosystem),
                "treasury_ecosystem_balance",
            )?,
            treasury_risk_reserve_balance: to_u64(
                self.treasury.balance_of(TreasuryAccountKind::RiskReserve),
                "treasury_risk_reserve_balance",
            )?,
            reserve_foreign_usdt_balance: to_u64(
                self.treasury.foreign_reserve("USDT"),
                "reserve_foreign_usdt_balance",
            )?,
            nav_soft_floor_value: to_u64(nav_soft_floor_value, "nav_soft_floor_value")?,
            buyback_last_spent_stable: to_u64(
                buyback_last_spent_stable,
                "buyback_last_spent_stable",
            )?,
            buyback_last_burned_token: to_u64(
                buyback_last_burned_token,
                "buyback_last_burned_token",
            )?,
        };
        Ok(())
    }

    pub fn snapshot(&self) -> Web30MarketEngineSnapshot {
        self.snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MarketGovernancePolicy;

    #[test]
    fn test_market_engine_apply_policy() {
        let policy = MarketGovernancePolicy::default();
        let runtime = Web30MarketEngine::from_policy(&policy).expect("init market engine");
        let snap = runtime.snapshot();

        assert_eq!(snap.amm_swap_fee_bp, policy.amm.swap_fee_bp);
        assert_eq!(
            snap.cdp_min_collateral_ratio_bp,
            policy.cdp.min_collateral_ratio_bp
        );
        assert_eq!(
            snap.cdp_liquidation_threshold_bp,
            policy.cdp.liquidation_threshold_bp
        );
        assert_eq!(snap.cdp_stability_fee_bp, policy.cdp.stability_fee_bp);
        assert_eq!(snap.bond_one_year_coupon_bp, policy.bond.coupon_rate_bp);
        assert_eq!(
            snap.nav_settlement_delay_epochs,
            policy.nav.settlement_delay_epochs
        );
        assert!(snap.treasury_main_balance > 0);
        assert!(snap.treasury_risk_reserve_balance > 0);
        assert!(snap.reserve_foreign_usdt_balance > 0);
        assert!(snap.nav_soft_floor_value > 0);
    }

    #[test]
    fn test_market_engine_reconfigure_updates_snapshot() {
        let mut runtime =
            Web30MarketEngine::from_policy(&MarketGovernancePolicy::default()).expect("init");
        let mut next = MarketGovernancePolicy::default();
        next.amm.swap_fee_bp = 45;
        next.cdp.min_collateral_ratio_bp = 16_000;
        next.cdp.liquidation_threshold_bp = 13_000;
        next.bond.coupon_rate_bp = 1_000;
        next.nav.settlement_delay_epochs = 5;
        runtime.reconfigure(&next).expect("reconfigure");
        let snap = runtime.snapshot();

        assert_eq!(snap.amm_swap_fee_bp, 45);
        assert_eq!(snap.cdp_min_collateral_ratio_bp, 16_000);
        assert_eq!(snap.cdp_liquidation_threshold_bp, 13_000);
        assert_eq!(snap.bond_one_year_coupon_bp, 1_000);
        assert_eq!(snap.nav_settlement_delay_epochs, 5);
        assert!(snap.treasury_main_balance > 0);
        assert!(snap.nav_soft_floor_value > 0);
    }

    #[test]
    fn test_market_engine_rejects_clamped_policy_values() {
        let mut bad = MarketGovernancePolicy::default();
        bad.cdp.liquidation_penalty_bp = 6_001;
        let err = match Web30MarketEngine::from_policy(&bad) {
            Ok(_) => panic!("expected cdp policy bound error"),
            Err(err) => err,
        };
        assert!(err
            .to_string()
            .contains("cdp.liquidation_penalty_bp must be <= 5000"));

        let mut bad_bond = MarketGovernancePolicy::default();
        bad_bond.bond.coupon_rate_bp = 6_000;
        let err = match Web30MarketEngine::from_policy(&bad_bond) {
            Ok(_) => panic!("expected bond policy bound error"),
            Err(err) => err,
        };
        assert!(err
            .to_string()
            .contains("bond.coupon_rate_bp must be <= 5000"));
    }
}
