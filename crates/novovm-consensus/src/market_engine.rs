use crate::types::{BFTError, BFTResult, MarketGovernancePolicy};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use web30_core::amm::AMMManager;
use web30_core::bonds::BondManager;
use web30_core::cdp::{CdpManager, CollateralType};
use web30_core::dividend_pool::{DividendEvent, DividendPool, DividendPoolImpl};
use web30_core::foreign_payment::{
    ForeignPayment, ForeignPaymentProcessor, ServiceType, FOREIGN_RATE_SCALE,
};
use web30_core::foreign_payment_impl::{
    ConfigurableExchangeRateProvider, ForeignPaymentConfig, ForeignPaymentProcessorImpl,
};
use web30_core::nav_redemption::NavRedemptionManager;
use web30_core::treasury::{Treasury, TreasuryAccountKind, TreasuryEvent};
use web30_core::treasury_impl::{BuybackConfig, NavConfig, TreasuryConfig, TreasuryImpl};
use web30_core::types::Address as Web30Address;

const ADDR_DOMAIN_SYSTEM: u8 = 0xC1;
const SYS_ADDR_TREASURY_CONTROLLER: u8 = 0xE0;
const SYS_ADDR_TREASURY_INGRESS: u8 = 0xE1;
const SYS_ADDR_MARKET_ORACLE: u8 = 0xE2;
const SYS_ADDR_NAV_REDEEMER: u8 = 0xE3;
const SYS_ADDR_DIVIDEND_PROBE_USER: u8 = 0xE4;
const SYS_ADDR_FOREIGN_TREASURY: u8 = 0xE5;
const SYS_ADDR_FOREIGN_M0_POOL: u8 = 0xE6;
const SYS_ADDR_FOREIGN_PROBE_MINER: u8 = 0xE7;
const DIVIDEND_PROBE_RING_SIZE: u8 = 32;
const DIVIDEND_MIN_BALANCE: u128 = 100;
const NAV_PRICE_BP_SCALE: u128 = 10_000;
const NAV_DEFAULT_PRICE_BP: u32 = 10_000;
const NAV_MIN_PRICE_BP: u32 = 1;
const NAV_MAX_PRICE_BP: u32 = 1_000_000;

fn system_address(tag: u8) -> Web30Address {
    let mut bytes = [0u8; 32];
    bytes[0] = tag;
    bytes[31] = ADDR_DOMAIN_SYSTEM;
    Web30Address::from_bytes(bytes)
}

fn dividend_probe_ring() -> Vec<Web30Address> {
    (0..DIVIDEND_PROBE_RING_SIZE)
        .map(|offset| system_address(SYS_ADDR_DIVIDEND_PROBE_USER.wrapping_add(offset)))
        .collect()
}

fn from_web30_error(ctx: &str, err: impl std::fmt::Display) -> BFTError {
    BFTError::InvalidProposal(format!("{}: {}", ctx, err))
}

fn to_u64(value: u128, ctx: &str) -> BFTResult<u64> {
    u64::try_from(value).map_err(|_| BFTError::Internal(format!("{} out of u64 range", ctx)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavValuationMode {
    Deterministic,
    ExternalFeed,
}

#[derive(Debug, Clone)]
struct ConfigurableNavValuationSource {
    mode: NavValuationMode,
    source_name: String,
    // price in basis points, where 10_000 == 1.0x
    external_price_bp: Option<u32>,
}

impl ConfigurableNavValuationSource {
    fn deterministic_v1() -> Self {
        Self {
            mode: NavValuationMode::Deterministic,
            source_name: "deterministic_v1".to_string(),
            external_price_bp: Some(NAV_DEFAULT_PRICE_BP),
        }
    }

    fn set_external_mode(&mut self, source_name: &str) -> BFTResult<()> {
        let name = source_name.trim();
        if name.is_empty() {
            return Err(BFTError::InvalidProposal(
                "nav valuation source name cannot be empty".to_string(),
            ));
        }
        self.mode = NavValuationMode::ExternalFeed;
        self.source_name = name.to_string();
        self.external_price_bp = None;
        Ok(())
    }

    fn set_external_price_bp(&mut self, price_bp: u32) -> BFTResult<()> {
        if !(NAV_MIN_PRICE_BP..=NAV_MAX_PRICE_BP).contains(&price_bp) {
            return Err(BFTError::InvalidProposal(format!(
                "nav valuation price_bp must be in [{}..{}], got {}",
                NAV_MIN_PRICE_BP, NAV_MAX_PRICE_BP, price_bp
            )));
        }
        self.external_price_bp = Some(price_bp);
        Ok(())
    }

    fn effective_price_bp(&self) -> (u32, bool) {
        match self.mode {
            NavValuationMode::Deterministic => (NAV_DEFAULT_PRICE_BP, false),
            NavValuationMode::ExternalFeed => match self.external_price_bp {
                Some(price) => (price, false),
                None => (NAV_DEFAULT_PRICE_BP, true),
            },
        }
    }

    fn source_name(&self) -> &str {
        &self.source_name
    }
}

#[derive(Debug, Clone)]
struct MarketOrchestrationOutcome {
    oracle_price_before: u128,
    oracle_price_after: u128,
    cdp_liquidation_candidates: u32,
    cdp_liquidations_executed: u32,
    cdp_liquidation_penalty_routed: u128,
    nav_snapshot_day: u64,
    nav_latest_value: u128,
    nav_valuation_source: String,
    nav_valuation_price_bp: u32,
    nav_valuation_fallback_used: bool,
    nav_redemptions_submitted: u32,
    nav_redemptions_executed: u32,
    nav_executed_stable_total: u128,
    dividend_income_received: u128,
    dividend_runtime_balance_accounts: u32,
    dividend_eligible_accounts: u32,
    dividend_snapshot_created: u32,
    dividend_claims_executed: u32,
    dividend_pool_balance: u128,
    foreign_payments_processed: u32,
    foreign_rate_source: String,
    foreign_rate_quote_spec_applied: bool,
    foreign_rate_fallback_used: bool,
    foreign_token_paid_total: u128,
    foreign_reserve_btc: u128,
    foreign_reserve_eth: u128,
    foreign_payment_reserve_usdt: u128,
    foreign_swap_out_total: u128,
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
    pub oracle_price_before: u64,
    pub oracle_price_after: u64,
    pub cdp_liquidation_candidates: u32,
    pub cdp_liquidations_executed: u32,
    pub cdp_liquidation_penalty_routed: u64,
    pub nav_snapshot_day: u64,
    pub nav_latest_value: u64,
    pub nav_valuation_source: String,
    pub nav_valuation_price_bp: u32,
    pub nav_valuation_fallback_used: bool,
    pub nav_redemptions_submitted: u32,
    pub nav_redemptions_executed: u32,
    pub nav_executed_stable_total: u64,
    pub dividend_income_received: u64,
    pub dividend_runtime_balance_accounts: u32,
    pub dividend_eligible_accounts: u32,
    pub dividend_snapshot_created: u32,
    pub dividend_claims_executed: u32,
    pub dividend_pool_balance: u64,
    pub foreign_payments_processed: u32,
    pub foreign_rate_source: String,
    pub foreign_rate_quote_spec_applied: bool,
    pub foreign_rate_fallback_used: bool,
    pub foreign_token_paid_total: u64,
    pub foreign_reserve_btc: u64,
    pub foreign_reserve_eth: u64,
    pub foreign_payment_reserve_usdt: u64,
    pub foreign_swap_out_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Web30MarketDividendBalanceSnapshot {
    pub address_hex: String,
    pub balance: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Web30MarketEngineStateSnapshot {
    pub snapshot: Web30MarketEngineSnapshot,
    pub dividend_runtime_balances: Vec<Web30MarketDividendBalanceSnapshot>,
    pub nav_valuation_external_mode: bool,
    pub nav_valuation_source_name: String,
    pub nav_valuation_external_price_bp: Option<u32>,
    pub foreign_rate_quote_spec_applied: bool,
    pub foreign_rate_fallback_used: bool,
    pub orchestration_day: u64,
}

fn address_to_hex(address: &Web30Address) -> String {
    let mut out = String::with_capacity(address.as_bytes().len() * 2);
    for byte in address.as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn address_from_hex(raw: &str) -> BFTResult<Web30Address> {
    let normalized = raw
        .trim()
        .strip_prefix("0x")
        .or_else(|| raw.trim().strip_prefix("0X"))
        .unwrap_or(raw.trim());
    if normalized.len() != 64 {
        return Err(BFTError::Internal(format!(
            "market engine address hex must be 64 chars, got {}",
            normalized.len()
        )));
    }
    let mut bytes = [0u8; 32];
    for (idx, pair) in normalized.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(pair).map_err(|_| {
            BFTError::Internal("market engine address hex contains invalid utf8".to_string())
        })?;
        bytes[idx] = u8::from_str_radix(hex, 16).map_err(|_| {
            BFTError::Internal(format!(
                "market engine address hex contains invalid byte {}",
                hex
            ))
        })?;
    }
    Ok(Web30Address::from_bytes(bytes))
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
    dividend: DividendPoolImpl,
    foreign_payment: ForeignPaymentProcessorImpl<ConfigurableExchangeRateProvider>,
    treasury: TreasuryImpl,
    #[allow(dead_code)]
    market_oracle: Web30Address,
    nav_redeemer: Web30Address,
    #[allow(dead_code)]
    treasury_controller: Web30Address,
    treasury_ingress: Web30Address,
    dividend_probe_user: Web30Address,
    foreign_probe_miner: Web30Address,
    // Snapshot sourced from unified account index service (kept deterministic, sorted).
    dividend_runtime_balances: Vec<(Web30Address, u128)>,
    foreign_rate_quote_spec_applied: bool,
    foreign_rate_fallback_used: bool,
    nav_valuation_source: ConfigurableNavValuationSource,
    orchestration_day: u64,
    snapshot: Web30MarketEngineSnapshot,
}

impl Web30MarketEngine {
    fn build_foreign_rate_provider(
        policy: &MarketGovernancePolicy,
    ) -> BFTResult<ConfigurableExchangeRateProvider> {
        let mut provider = ConfigurableExchangeRateProvider::deterministic_v1();
        provider
            .set_source_name("market_policy_config_v1")
            .map_err(|e| from_web30_error("market engine foreign rate source", e))?;
        // Keep USDT leg deterministic but policy-aware for spread/volatility simulation.
        // This is a configurable source entrypoint (can be overridden by upper-layer config).
        let usdt_slippage =
            (u32::from(policy.reserve.redemption_fee_bp) / 10).clamp(10, 500) as u16;
        provider
            .set_rate_scaled_with_slippage("USDT", 10u128 * FOREIGN_RATE_SCALE, usdt_slippage)
            .map_err(|e| from_web30_error("market engine foreign usdt quote", e))?;
        Ok(provider)
    }

    fn id_with_prefix(prefix: u8, day: u64) -> [u8; 32] {
        let mut out = [0u8; 32];
        out[0] = prefix;
        out[1..9].copy_from_slice(&day.to_le_bytes());
        out[31] = ADDR_DOMAIN_SYSTEM;
        out
    }

    fn default_dividend_seed_balances(
        probe_ring: &[Web30Address],
        dividend_probe_user: Web30Address,
        foreign_probe_miner: Web30Address,
    ) -> Vec<(Web30Address, u128)> {
        let mut balances: Vec<(Web30Address, u128)> = probe_ring
            .iter()
            .cloned()
            .map(|addr| (addr, 1_000))
            .collect();
        // Keep historical probe identities present in snapshots for compatibility.
        balances.push((dividend_probe_user, 2_000));
        balances.push((foreign_probe_miner, 1_000));
        balances
    }

    fn merge_dividend_seed_balances(
        runtime_balances: &[(Web30Address, u128)],
        probe_ring: &[Web30Address],
        dividend_probe_user: Web30Address,
        foreign_probe_miner: Web30Address,
    ) -> Vec<(Web30Address, u128)> {
        let mut merged: HashMap<Web30Address, u128> = Self::default_dividend_seed_balances(
            probe_ring,
            dividend_probe_user,
            foreign_probe_miner,
        )
        .into_iter()
        .collect();
        // Runtime/token balances override deterministic probe seed when same account appears.
        for (addr, amount) in runtime_balances {
            merged.insert(*addr, *amount);
        }
        let mut out: Vec<(Web30Address, u128)> = merged.into_iter().collect();
        out.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
        out
    }

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
            buyback_config: BuybackConfig {
                min_main_reserve_bp: policy.reserve.min_reserve_ratio_bp,
                burn_share_bp: policy.buyback.burn_share_bp,
                trigger_discount_bp: policy.buyback.trigger_discount_bp,
                observed_market_discount_bp: policy
                    .buyback
                    .trigger_discount_bp
                    .saturating_add(50)
                    .min(10_000),
                orderbook_liquidity_stable: u128::from(
                    policy.buyback.max_treasury_budget_per_epoch,
                )
                .saturating_mul(4),
                amm_liquidity_stable: u128::from(policy.buyback.max_treasury_budget_per_epoch)
                    .saturating_mul(4),
                max_execution_slippage_bp: 1_500,
            },
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
        let market_oracle = system_address(SYS_ADDR_MARKET_ORACLE);
        let nav_redeemer = system_address(SYS_ADDR_NAV_REDEEMER);
        let dividend_probe_user = system_address(SYS_ADDR_DIVIDEND_PROBE_USER);
        let foreign_treasury = system_address(SYS_ADDR_FOREIGN_TREASURY);
        let foreign_m0_pool = system_address(SYS_ADDR_FOREIGN_M0_POOL);
        let foreign_probe_miner = system_address(SYS_ADDR_FOREIGN_PROBE_MINER);

        let mut runtime = Self {
            amm: AMMManager::new(),
            cdp: CdpManager::new(policy.cdp.liquidation_penalty_bp),
            bond: BondManager::new(),
            nav: NavRedemptionManager::new(
                u128::from(policy.nav.max_daily_redemption_bp),
                policy.reserve.min_reserve_ratio_bp,
                u64::from(policy.nav.settlement_delay_epochs),
            ),
            dividend: DividendPoolImpl::new(DIVIDEND_MIN_BALANCE),
            foreign_payment: ForeignPaymentProcessorImpl::new(ForeignPaymentConfig {
                rate_provider: Self::build_foreign_rate_provider(policy)?,
                treasury_address: foreign_treasury,
                m0_pool_address: foreign_m0_pool,
            }),
            treasury: Self::build_treasury(policy, treasury_controller, None),
            market_oracle,
            nav_redeemer,
            treasury_controller,
            treasury_ingress,
            dividend_probe_user,
            foreign_probe_miner,
            dividend_runtime_balances: Vec::new(),
            foreign_rate_quote_spec_applied: false,
            foreign_rate_fallback_used: false,
            nav_valuation_source: ConfigurableNavValuationSource::deterministic_v1(),
            orchestration_day: 0,
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
                oracle_price_before: 0,
                oracle_price_after: 0,
                cdp_liquidation_candidates: 0,
                cdp_liquidations_executed: 0,
                cdp_liquidation_penalty_routed: 0,
                nav_snapshot_day: 0,
                nav_latest_value: 0,
                nav_valuation_source: String::new(),
                nav_valuation_price_bp: 0,
                nav_valuation_fallback_used: false,
                nav_redemptions_submitted: 0,
                nav_redemptions_executed: 0,
                nav_executed_stable_total: 0,
                dividend_income_received: 0,
                dividend_runtime_balance_accounts: 0,
                dividend_eligible_accounts: 0,
                dividend_snapshot_created: 0,
                dividend_claims_executed: 0,
                dividend_pool_balance: 0,
                foreign_payments_processed: 0,
                foreign_rate_source: String::new(),
                foreign_rate_quote_spec_applied: false,
                foreign_rate_fallback_used: false,
                foreign_token_paid_total: 0,
                foreign_reserve_btc: 0,
                foreign_reserve_eth: 0,
                foreign_payment_reserve_usdt: 0,
                foreign_swap_out_total: 0,
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
        self.orchestration_day = self.orchestration_day.saturating_add(1);
        let orchestration = self.run_cross_module_orchestration(policy, self.orchestration_day)?;
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
            oracle_price_before: to_u64(orchestration.oracle_price_before, "oracle_price_before")?,
            oracle_price_after: to_u64(orchestration.oracle_price_after, "oracle_price_after")?,
            cdp_liquidation_candidates: orchestration.cdp_liquidation_candidates,
            cdp_liquidations_executed: orchestration.cdp_liquidations_executed,
            cdp_liquidation_penalty_routed: to_u64(
                orchestration.cdp_liquidation_penalty_routed,
                "cdp_liquidation_penalty_routed",
            )?,
            nav_snapshot_day: orchestration.nav_snapshot_day,
            nav_latest_value: to_u64(orchestration.nav_latest_value, "nav_latest_value")?,
            nav_valuation_source: orchestration.nav_valuation_source,
            nav_valuation_price_bp: orchestration.nav_valuation_price_bp,
            nav_valuation_fallback_used: orchestration.nav_valuation_fallback_used,
            nav_redemptions_submitted: orchestration.nav_redemptions_submitted,
            nav_redemptions_executed: orchestration.nav_redemptions_executed,
            nav_executed_stable_total: to_u64(
                orchestration.nav_executed_stable_total,
                "nav_executed_stable_total",
            )?,
            dividend_income_received: to_u64(
                orchestration.dividend_income_received,
                "dividend_income_received",
            )?,
            dividend_runtime_balance_accounts: orchestration.dividend_runtime_balance_accounts,
            dividend_eligible_accounts: orchestration.dividend_eligible_accounts,
            dividend_snapshot_created: orchestration.dividend_snapshot_created,
            dividend_claims_executed: orchestration.dividend_claims_executed,
            dividend_pool_balance: to_u64(
                orchestration.dividend_pool_balance,
                "dividend_pool_balance",
            )?,
            foreign_payments_processed: orchestration.foreign_payments_processed,
            foreign_rate_source: orchestration.foreign_rate_source,
            foreign_rate_quote_spec_applied: orchestration.foreign_rate_quote_spec_applied,
            foreign_rate_fallback_used: orchestration.foreign_rate_fallback_used,
            foreign_token_paid_total: to_u64(
                orchestration.foreign_token_paid_total,
                "foreign_token_paid_total",
            )?,
            foreign_reserve_btc: to_u64(orchestration.foreign_reserve_btc, "foreign_reserve_btc")?,
            foreign_reserve_eth: to_u64(orchestration.foreign_reserve_eth, "foreign_reserve_eth")?,
            foreign_payment_reserve_usdt: to_u64(
                orchestration.foreign_payment_reserve_usdt,
                "foreign_payment_reserve_usdt",
            )?,
            foreign_swap_out_total: to_u64(
                orchestration.foreign_swap_out_total,
                "foreign_swap_out_total",
            )?,
        };
        Ok(())
    }

    /// Set unified account-index snapshot for dividend path.
    /// Market engine merges these balances with deterministic probe accounts.
    pub fn set_dividend_account_index_snapshot<I>(&mut self, balances: I)
    where
        I: IntoIterator<Item = (Web30Address, u128)>,
    {
        self.dividend_runtime_balances = balances.into_iter().collect();
        self.dividend_runtime_balances
            .sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
    }

    /// Compatibility shim for historical call sites/scripts.
    pub fn set_dividend_runtime_balances<I>(&mut self, balances: I)
    where
        I: IntoIterator<Item = (Web30Address, u128)>,
    {
        self.set_dividend_account_index_snapshot(balances);
    }

    /// Switch NAV valuation to external-feed mode.
    pub fn set_nav_valuation_source_external(&mut self, source_name: &str) -> BFTResult<()> {
        self.nav_valuation_source.set_external_mode(source_name)
    }

    /// Set external NAV valuation price in basis points (10_000 == 1.0x).
    pub fn set_nav_external_price_bp(&mut self, price_bp: u32) -> BFTResult<()> {
        self.nav_valuation_source.set_external_price_bp(price_bp)
    }

    /// Switch foreign exchange source label (for policy/audit trace).
    pub fn set_foreign_rate_source_name(&mut self, source_name: &str) -> BFTResult<()> {
        let normalized = source_name.trim().to_ascii_lowercase();
        self.foreign_payment
            .set_rate_source_name(source_name)
            .map_err(|e| from_web30_error("market engine foreign rate source", e))?;
        // External source without explicit quote means deterministic fallback remains active.
        self.foreign_rate_quote_spec_applied = false;
        self.foreign_rate_fallback_used = normalized != "market_policy_config_v1";
        Ok(())
    }

    /// Apply external foreign exchange quote spec to runtime processor.
    pub fn apply_foreign_quote_spec(&mut self, quote_spec: &str) -> BFTResult<()> {
        self.foreign_payment
            .apply_quote_spec(quote_spec)
            .map_err(|e| from_web30_error("market engine foreign quote spec", e))?;
        self.foreign_rate_quote_spec_applied = true;
        self.foreign_rate_fallback_used = false;
        Ok(())
    }

    fn run_cross_module_orchestration(
        &mut self,
        policy: &MarketGovernancePolicy,
        day: u64,
    ) -> BFTResult<MarketOrchestrationOutcome> {
        // Force dividend module to use consensus day source, not host clock.
        self.dividend.set_current_day_override(day);

        let debt: u128 = 10_000_000;
        let min_ratio_bp = u128::from(policy.cdp.min_collateral_ratio_bp.max(10_000));
        let oracle_price_before =
            (debt.saturating_mul(min_ratio_bp) / 10_000).saturating_add(1_000_000);
        let cdp_id = Self::id_with_prefix(0xA1, day);
        self.cdp
            .open_cdp(
                cdp_id,
                self.market_oracle,
                CollateralType::MainnetToken,
                1_000_000,
                oracle_price_before,
                debt,
                day,
            )
            .map_err(|e| from_web30_error("market engine cdp open", e))?;

        let treasury_balance_for_nav = self
            .treasury
            .balance_of(TreasuryAccountKind::Main)
            .saturating_add(self.treasury.balance_of(TreasuryAccountKind::Ecosystem))
            .saturating_add(self.treasury.balance_of(TreasuryAccountKind::RiskReserve));
        let reserve_ratio_bp = u128::from(policy.reserve.min_reserve_ratio_bp.max(1));
        let reserve_ratio_den = 10_000u128.saturating_sub(reserve_ratio_bp).max(1);
        let min_reserve_for_nav = treasury_balance_for_nav
            .saturating_mul(reserve_ratio_bp)
            .saturating_add(reserve_ratio_den.saturating_sub(1))
            / reserve_ratio_den;
        let (nav_valuation_price_bp, nav_valuation_fallback_used) =
            self.nav_valuation_source.effective_price_bp();
        let reserve_foreign_usdt = self.treasury.foreign_reserve("USDT");
        let external_reserve_value = reserve_foreign_usdt
            .saturating_mul(u128::from(nav_valuation_price_bp))
            / NAV_PRICE_BP_SCALE;
        let reserve_value = min_reserve_for_nav.max(external_reserve_value).max(1);
        let total_value_for_nav = reserve_value.saturating_add(treasury_balance_for_nav);
        let daily_quota = u128::from(policy.nav.max_daily_redemption_bp.max(1));
        let min_supply_for_quota =
            total_value_for_nav.saturating_add(daily_quota.saturating_sub(1)) / daily_quota;
        let circulating_supply = self.cdp.total_supply().max(min_supply_for_quota).max(1);
        let nav_snapshot = self
            .nav
            .record_nav_snapshot(
                day,
                reserve_value,
                treasury_balance_for_nav,
                circulating_supply,
            )
            .map_err(|e| from_web30_error("market engine nav snapshot", e))?;
        let nav_value = nav_snapshot.nav_value.max(1);
        let mut redemption_token_amount =
            (1_000_000u128.saturating_add(nav_value).saturating_sub(1)) / nav_value;
        if redemption_token_amount == 0 {
            redemption_token_amount = 1;
        }
        let mut redemption_expected_stable =
            redemption_token_amount.saturating_mul(nav_value) / 1_000_000;
        if redemption_expected_stable == 0 {
            redemption_token_amount = redemption_token_amount.saturating_add(1);
            redemption_expected_stable =
                redemption_token_amount.saturating_mul(nav_value) / 1_000_000;
        }
        if redemption_expected_stable > daily_quota {
            redemption_token_amount = daily_quota.saturating_mul(1_000_000) / nav_value;
            if redemption_token_amount == 0 {
                redemption_token_amount = 1;
            }
        }
        let redemption_id = Self::id_with_prefix(0xB1, day);
        self.nav
            .submit_redemption(
                redemption_id,
                self.nav_redeemer,
                redemption_token_amount,
                day,
            )
            .map_err(|e| from_web30_error("market engine nav redemption submit", e))?;
        let nav_redemptions_submitted = 1u32;
        let redemption_exec_day = day.saturating_add(u64::from(policy.nav.settlement_delay_epochs));
        let executed_redemptions = self
            .nav
            .process_redemptions(redemption_exec_day)
            .map_err(|e| from_web30_error("market engine nav redemption execute", e))?;
        let nav_executed_stable_total: u128 = executed_redemptions
            .iter()
            .map(|req| req.expected_stable)
            .sum();
        let nav_redemptions_executed = u32::try_from(executed_redemptions.len())
            .map_err(|_| BFTError::Internal("nav_redemptions_executed overflow".to_string()))?;

        let liquidation_threshold_bp = u128::from(policy.cdp.liquidation_threshold_bp.max(10_000));
        let oracle_price_after =
            (debt.saturating_mul(liquidation_threshold_bp.saturating_sub(1)) / 10_000).max(1);
        self.cdp
            .update_collateral_price(cdp_id, oracle_price_after)
            .map_err(|e| from_web30_error("market engine oracle price update", e))?;

        let liquidatable = self.cdp.find_liquidatable_cdps();
        let cdp_liquidation_candidates = u32::try_from(liquidatable.len())
            .map_err(|_| BFTError::Internal("cdp_liquidation_candidates overflow".to_string()))?;
        let mut cdp_liquidation_penalty_routed = 0u128;
        let mut cdp_liquidations_executed = 0u32;
        for liquidatable_id in liquidatable {
            let (_seized, penalty) = self
                .cdp
                .liquidate_cdp(liquidatable_id)
                .map_err(|e| from_web30_error("market engine cdp liquidation", e))?;
            cdp_liquidation_penalty_routed = cdp_liquidation_penalty_routed.saturating_add(penalty);
            cdp_liquidations_executed = cdp_liquidations_executed.saturating_add(1);
        }
        if cdp_liquidation_penalty_routed > 0 {
            self.treasury
                .on_income(
                    &self.treasury_ingress,
                    cdp_liquidation_penalty_routed,
                    TreasuryAccountKind::RiskReserve,
                )
                .map_err(|e| from_web30_error("market engine liquidation penalty route", e))?;
        }

        let dividend_income_received = u128::from(policy.reserve.redemption_fee_bp.max(1));
        self.dividend
            .receive_income(&self.treasury_ingress, dividend_income_received)
            .map_err(|e| from_web30_error("market engine dividend income", e))?;
        let probe_ring = dividend_probe_ring();
        let seed_balances = Self::merge_dividend_seed_balances(
            &self.dividend_runtime_balances,
            &probe_ring,
            self.dividend_probe_user,
            self.foreign_probe_miner,
        );
        let dividend_runtime_balance_accounts = u32::try_from(self.dividend_runtime_balances.len())
            .map_err(|_| {
                BFTError::Internal("dividend_runtime_balance_accounts overflow".to_string())
            })?;
        let dividend_eligible_accounts = u32::try_from(
            seed_balances
                .iter()
                .filter(|(_, amount)| *amount >= DIVIDEND_MIN_BALANCE)
                .count(),
        )
        .map_err(|_| BFTError::Internal("dividend_eligible_accounts overflow".to_string()))?;
        self.dividend.set_account_balances(seed_balances);
        let dividend_today = self.dividend.current_day();
        let dividend_snapshot_created = if self.dividend.get_snapshot(dividend_today).is_some() {
            1
        } else {
            match self
                .dividend
                .take_daily_snapshot()
                .map_err(|e| from_web30_error("market engine dividend snapshot", e))?
            {
                DividendEvent::SnapshotCreated { .. } => 1,
                other => {
                    return Err(BFTError::Internal(format!(
                        "market engine dividend snapshot returned unexpected event: {:?}",
                        other
                    )));
                }
            }
        };
        let probe_index = day.saturating_sub(1) as usize % probe_ring.len();
        let probe_user = probe_ring[probe_index];
        let dividend_claims_executed = if self.dividend.claim(&probe_user).is_ok() {
            1
        } else {
            0
        };
        let dividend_pool_balance = self.dividend.pool_balance();

        let mut foreign_payments_processed = 0u32;
        let mut foreign_token_paid_total = 0u128;
        let foreign_payments = vec![
            ForeignPayment {
                currency: "BTC".to_string(),
                amount: 100_000,
                payer: "btc_probe".to_string(),
                service_type: ServiceType::Gas,
            },
            ForeignPayment {
                currency: "ETH".to_string(),
                amount: 1_000_000_000_000_000,
                payer: "eth_probe".to_string(),
                service_type: ServiceType::TransactionFee,
            },
            ForeignPayment {
                currency: "USDT".to_string(),
                amount: 500_000,
                payer: "usdt_probe".to_string(),
                service_type: ServiceType::CrossChainBridge,
            },
        ];
        for payment in foreign_payments {
            let receipt = self
                .foreign_payment
                .process_foreign_payment(payment, self.foreign_probe_miner)
                .map_err(|e| from_web30_error("market engine foreign payment process", e))?;
            foreign_payments_processed = foreign_payments_processed.saturating_add(1);
            foreign_token_paid_total =
                foreign_token_paid_total.saturating_add(receipt.token_amount);
        }
        let foreign_swap_out_total = self
            .foreign_payment
            .miner_swap_to_foreign(self.foreign_probe_miner, 1_000, "USDT", 1)
            .map_err(|e| from_web30_error("market engine foreign swap", e))?;
        let foreign_stats = self.foreign_payment.stats();
        let foreign_reserve_btc = foreign_stats
            .current_reserves
            .get("BTC")
            .copied()
            .unwrap_or(0);
        let foreign_reserve_eth = foreign_stats
            .current_reserves
            .get("ETH")
            .copied()
            .unwrap_or(0);
        let foreign_payment_reserve_usdt = foreign_stats
            .current_reserves
            .get("USDT")
            .copied()
            .unwrap_or(0);
        let foreign_rate_source = self.foreign_payment.rate_source_name().to_string();
        let foreign_rate_quote_spec_applied = self.foreign_rate_quote_spec_applied;
        let foreign_rate_fallback_used = self.foreign_rate_fallback_used;

        Ok(MarketOrchestrationOutcome {
            oracle_price_before,
            oracle_price_after,
            cdp_liquidation_candidates,
            cdp_liquidations_executed,
            cdp_liquidation_penalty_routed,
            nav_snapshot_day: nav_snapshot.day,
            nav_latest_value: nav_snapshot.nav_value,
            nav_valuation_source: self.nav_valuation_source.source_name().to_string(),
            nav_valuation_price_bp,
            nav_valuation_fallback_used,
            nav_redemptions_submitted,
            nav_redemptions_executed,
            nav_executed_stable_total,
            dividend_income_received,
            dividend_runtime_balance_accounts,
            dividend_eligible_accounts,
            dividend_snapshot_created,
            dividend_claims_executed,
            dividend_pool_balance,
            foreign_payments_processed,
            foreign_rate_source,
            foreign_rate_quote_spec_applied,
            foreign_rate_fallback_used,
            foreign_token_paid_total,
            foreign_reserve_btc,
            foreign_reserve_eth,
            foreign_payment_reserve_usdt,
            foreign_swap_out_total,
        })
    }

    pub fn snapshot(&self) -> Web30MarketEngineSnapshot {
        self.snapshot.clone()
    }

    pub fn state_snapshot(&self) -> Web30MarketEngineStateSnapshot {
        let mut dividend_runtime_balances: Vec<_> = self
            .dividend_runtime_balances
            .iter()
            .map(|(address, balance)| Web30MarketDividendBalanceSnapshot {
                address_hex: address_to_hex(address),
                balance: *balance,
            })
            .collect();
        dividend_runtime_balances.sort_by(|left, right| left.address_hex.cmp(&right.address_hex));
        Web30MarketEngineStateSnapshot {
            snapshot: self.snapshot.clone(),
            dividend_runtime_balances,
            nav_valuation_external_mode: self.nav_valuation_source.mode
                == NavValuationMode::ExternalFeed,
            nav_valuation_source_name: self.nav_valuation_source.source_name.clone(),
            nav_valuation_external_price_bp: self.nav_valuation_source.external_price_bp,
            foreign_rate_quote_spec_applied: self.foreign_rate_quote_spec_applied,
            foreign_rate_fallback_used: self.foreign_rate_fallback_used,
            orchestration_day: self.orchestration_day,
        }
    }

    pub fn restore_from_state_snapshot(
        policy: &MarketGovernancePolicy,
        state: &Web30MarketEngineStateSnapshot,
    ) -> BFTResult<Self> {
        let mut runtime = Self::from_policy(policy)?;
        let dividend_runtime_balances: Vec<(Web30Address, u128)> = state
            .dividend_runtime_balances
            .iter()
            .map(|entry| Ok((address_from_hex(&entry.address_hex)?, entry.balance)))
            .collect::<BFTResult<Vec<_>>>()?;
        runtime.set_dividend_account_index_snapshot(dividend_runtime_balances);
        runtime.treasury = Self::build_treasury(
            policy,
            runtime.treasury_controller,
            Some((
                u128::from(state.snapshot.treasury_main_balance),
                u128::from(state.snapshot.treasury_ecosystem_balance),
                u128::from(state.snapshot.treasury_risk_reserve_balance),
            )),
        );
        for (ticker, amount) in [
            ("USDT", state.snapshot.reserve_foreign_usdt_balance),
            ("BTC", state.snapshot.foreign_reserve_btc),
            ("ETH", state.snapshot.foreign_reserve_eth),
        ] {
            if amount > 0 {
                runtime
                    .treasury
                    .collect_foreign_currency(ticker.to_string(), u128::from(amount))
                    .map_err(|e| from_web30_error("market engine restore foreign reserve", e))?;
            }
        }
        if state.nav_valuation_external_mode {
            runtime.set_nav_valuation_source_external(&state.nav_valuation_source_name)?;
            if let Some(price_bp) = state.nav_valuation_external_price_bp {
                runtime.set_nav_external_price_bp(price_bp)?;
            }
        }
        if !state.snapshot.foreign_rate_source.trim().is_empty() {
            runtime.set_foreign_rate_source_name(&state.snapshot.foreign_rate_source)?;
        }
        runtime.foreign_rate_quote_spec_applied = state.foreign_rate_quote_spec_applied;
        runtime.foreign_rate_fallback_used = state.foreign_rate_fallback_used;
        runtime.orchestration_day = state.orchestration_day;
        runtime.snapshot = state.snapshot.clone();
        Ok(runtime)
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
        assert!(snap.oracle_price_before > snap.oracle_price_after);
        assert!(snap.cdp_liquidation_candidates > 0);
        assert!(snap.cdp_liquidations_executed > 0);
        assert!(snap.cdp_liquidation_penalty_routed > 0);
        assert!(snap.nav_snapshot_day > 0);
        assert!(snap.nav_latest_value > 0);
        assert_eq!(snap.nav_valuation_source, "deterministic_v1");
        assert_eq!(snap.nav_valuation_price_bp, NAV_DEFAULT_PRICE_BP);
        assert!(!snap.nav_valuation_fallback_used);
        assert!(snap.nav_redemptions_submitted > 0);
        assert!(snap.nav_redemptions_executed > 0);
        assert!(snap.nav_executed_stable_total > 0);
        assert!(snap.dividend_income_received > 0);
        assert!(snap.dividend_eligible_accounts > 0);
        assert!(snap.dividend_snapshot_created > 0);
        assert!(snap.dividend_claims_executed > 0);
        assert!(snap.dividend_pool_balance > 0);
        assert!(snap.foreign_payments_processed > 0);
        assert_eq!(snap.foreign_rate_source, "market_policy_config_v1");
        assert!(!snap.foreign_rate_quote_spec_applied);
        assert!(!snap.foreign_rate_fallback_used);
        assert!(snap.foreign_token_paid_total > 0);
        assert!(snap.foreign_reserve_btc > 0);
        assert!(snap.foreign_reserve_eth > 0);
        assert!(snap.foreign_payment_reserve_usdt > 0);
        assert!(snap.foreign_swap_out_total > 0);
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
        assert!(snap.cdp_liquidations_executed > 0);
        assert!(snap.nav_redemptions_executed > 0);
        assert_eq!(snap.nav_valuation_source, "deterministic_v1");
        assert_eq!(snap.nav_valuation_price_bp, NAV_DEFAULT_PRICE_BP);
        assert!(!snap.nav_valuation_fallback_used);
        assert!(snap.dividend_income_received > 0);
        assert!(snap.dividend_eligible_accounts > 0);
        assert!(snap.dividend_snapshot_created > 0);
        assert!(snap.dividend_claims_executed > 0);
        assert!(snap.dividend_pool_balance > 0);
        assert!(snap.foreign_payments_processed > 0);
        assert_eq!(snap.foreign_rate_source, "market_policy_config_v1");
        assert!(!snap.foreign_rate_quote_spec_applied);
        assert!(!snap.foreign_rate_fallback_used);
        assert!(snap.foreign_token_paid_total > 0);
        assert!(snap.foreign_reserve_btc > 0);
        assert!(snap.foreign_reserve_eth > 0);
        assert!(snap.foreign_payment_reserve_usdt > 0);
        assert!(snap.foreign_swap_out_total > 0);
    }

    #[test]
    fn test_market_engine_uses_runtime_dividend_balance_seed() {
        let mut runtime =
            Web30MarketEngine::from_policy(&MarketGovernancePolicy::default()).expect("init");
        let holder_a = system_address(0x71);
        let holder_b = system_address(0x72);
        runtime.set_dividend_runtime_balances(vec![(holder_a, 5_000), (holder_b, 8_000)]);
        runtime
            .reconfigure(&MarketGovernancePolicy::default())
            .expect("reconfigure");
        let snap = runtime.snapshot();

        assert!(snap.dividend_runtime_balance_accounts >= 2);
        assert!(snap.dividend_eligible_accounts >= 2);
        assert!(snap.dividend_claims_executed > 0);
    }

    #[test]
    fn test_nav_valuation_source_external_with_price() {
        let mut runtime =
            Web30MarketEngine::from_policy(&MarketGovernancePolicy::default()).expect("init");
        runtime
            .set_nav_valuation_source_external("external_feed_v1")
            .expect("set source");
        runtime
            .set_nav_external_price_bp(12_000)
            .expect("set external price");
        runtime
            .reconfigure(&MarketGovernancePolicy::default())
            .expect("reconfigure");
        let snap = runtime.snapshot();

        assert_eq!(snap.nav_valuation_source, "external_feed_v1");
        assert_eq!(snap.nav_valuation_price_bp, 12_000);
        assert!(!snap.nav_valuation_fallback_used);
        assert!(snap.nav_latest_value > 0);
    }

    #[test]
    fn test_nav_valuation_source_external_missing_quote_fallback() {
        let mut runtime =
            Web30MarketEngine::from_policy(&MarketGovernancePolicy::default()).expect("init");
        runtime
            .set_nav_valuation_source_external("external_feed_no_quote")
            .expect("set source");
        runtime
            .reconfigure(&MarketGovernancePolicy::default())
            .expect("reconfigure");
        let snap = runtime.snapshot();

        assert_eq!(snap.nav_valuation_source, "external_feed_no_quote");
        assert_eq!(snap.nav_valuation_price_bp, NAV_DEFAULT_PRICE_BP);
        assert!(snap.nav_valuation_fallback_used);
        assert!(snap.nav_latest_value > 0);
    }

    #[test]
    fn test_nav_valuation_source_reject_invalid_price() {
        let mut runtime =
            Web30MarketEngine::from_policy(&MarketGovernancePolicy::default()).expect("init");
        runtime
            .set_nav_valuation_source_external("external_feed_v1")
            .expect("set source");
        let err = runtime
            .set_nav_external_price_bp(0)
            .expect_err("invalid price should fail");
        assert!(err
            .to_string()
            .contains("nav valuation price_bp must be in [1..1000000]"));
    }

    #[test]
    fn test_foreign_rate_source_external_with_quote_spec() {
        let mut runtime =
            Web30MarketEngine::from_policy(&MarketGovernancePolicy::default()).expect("init");
        runtime
            .set_foreign_rate_source_name("external_feed_v1")
            .expect("set foreign source");
        runtime
            .apply_foreign_quote_spec("BTC:120000:90,ETH:6000:70,USDT:10:20")
            .expect("apply quote spec");
        runtime
            .reconfigure(&MarketGovernancePolicy::default())
            .expect("reconfigure");
        let snap = runtime.snapshot();

        assert_eq!(snap.foreign_rate_source, "external_feed_v1");
        assert!(snap.foreign_rate_quote_spec_applied);
        assert!(!snap.foreign_rate_fallback_used);
        assert!(snap.foreign_payments_processed > 0);
    }

    #[test]
    fn test_foreign_rate_source_external_missing_quote_fallback() {
        let mut runtime =
            Web30MarketEngine::from_policy(&MarketGovernancePolicy::default()).expect("init");
        runtime
            .set_foreign_rate_source_name("external_feed_no_quote")
            .expect("set source");
        runtime
            .reconfigure(&MarketGovernancePolicy::default())
            .expect("reconfigure");
        let snap = runtime.snapshot();

        assert_eq!(snap.foreign_rate_source, "external_feed_no_quote");
        assert!(!snap.foreign_rate_quote_spec_applied);
        assert!(snap.foreign_rate_fallback_used);
        assert!(snap.foreign_payments_processed > 0);
    }

    #[test]
    fn test_foreign_rate_source_reject_invalid_quote_spec() {
        let mut runtime =
            Web30MarketEngine::from_policy(&MarketGovernancePolicy::default()).expect("init");
        runtime
            .set_foreign_rate_source_name("external_feed_v1")
            .expect("set source");
        let err = runtime
            .apply_foreign_quote_spec("BTC:0:50")
            .expect_err("invalid quote spec should fail");
        let msg = err.to_string();
        assert!(
            msg.contains("rate") || msg.contains("invalid"),
            "unexpected error: {}",
            msg
        );
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

    #[test]
    fn test_market_engine_rejects_zero_buyback_budget() {
        let mut bad = MarketGovernancePolicy::default();
        bad.buyback.max_treasury_budget_per_epoch = 0;
        let err = match Web30MarketEngine::from_policy(&bad) {
            Ok(_) => panic!("expected buyback budget validation error"),
            Err(err) => err,
        };
        assert!(err
            .to_string()
            .contains("buyback.max_treasury_budget_per_epoch must be > 0"));
    }
}
