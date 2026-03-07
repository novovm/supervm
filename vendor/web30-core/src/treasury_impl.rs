//! SuperVM Treasury 参考实现
//!
//! 职责:
//! - 管理三类国库账户余额 (Main/Ecosystem/RiskReserve)
//! - 接收协议费用收入 (Gas/服务费路由过来的部分)
//! - 执行治理授权的支出 (生态投资/团队补贴等)
//! - 提供回购+销毁策略执行接口
//! - 外汇归集与矿工 Token 支付记录
//! - NAV 计算桩 (用于双轨定价机制的软下限)

use std::collections::HashMap;

use anyhow::{bail, Result};

use crate::{
    treasury::{Treasury, TreasuryAccountKind, TreasuryEvent},
    types::Address,
};

/// Treasury 初始化配置
pub struct TreasuryConfig {
    /// 初始余额分配 (账户类型 -> 金额)
    pub initial_balances: HashMap<TreasuryAccountKind, u128>,
    /// 治理控制者地址 (可为多签/DAO 合约)
    pub controller: Address,
    /// 回购与销毁执行参数
    pub buyback_config: BuybackConfig,
    /// NAV 计算相关参数
    pub nav_config: NavConfig,
}

/// NAV 计算配置
#[derive(Debug, Clone)]
pub struct NavConfig {
    /// 最小 NAV 倍数 (相对初始价格, 基点)
    pub min_nav_multiplier_bp: u16,
    /// 储备资产价值估算来源 (可由外部治理/运维标记)
    pub reserve_valuation_source: String,
}

impl Default for NavConfig {
    fn default() -> Self {
        Self {
            min_nav_multiplier_bp: 5000, // 0.5x 初始价格作为最低 NAV
            reserve_valuation_source: "deterministic_quote_v1".into(),
        }
    }
}

/// 回购与销毁执行配置
#[derive(Debug, Clone)]
pub struct BuybackConfig {
    /// Main 账户最低保留比例（bps）
    pub min_main_reserve_bp: u16,
    /// 回购后销毁比例（bps）
    pub burn_share_bp: u16,
    /// 触发折价阈值（bps）
    pub trigger_discount_bp: u16,
    /// 当前观测折价（bps，来自上游策略或风控输入）
    pub observed_market_discount_bp: u16,
}

impl Default for BuybackConfig {
    fn default() -> Self {
        Self {
            min_main_reserve_bp: 1000,
            burn_share_bp: 5000,
            trigger_discount_bp: 500,
            observed_market_discount_bp: 600,
        }
    }
}

/// Treasury 实现
pub struct TreasuryImpl {
    /// 各账户余额 (单位: 主链 Token 最小单位)
    balances: HashMap<TreasuryAccountKind, u128>,
    /// 治理控制者
    controller: Address,
    /// NAV 配置
    nav_config: NavConfig,
    /// 回购配置
    buyback_config: BuybackConfig,
    /// 累计收入统计 (各账户)
    total_income: HashMap<TreasuryAccountKind, u128>,
    /// 累计支出统计 (各账户)
    total_spent: HashMap<TreasuryAccountKind, u128>,
    /// 外汇储备统计 (币种 -> 金额, 单位: 各币种最小单位)
    foreign_reserves: HashMap<String, u128>,
    /// 已支付矿工的 Token 总量
    total_miner_paid: u128,
}

impl TreasuryImpl {
    fn normalize_bp(v: u16) -> u16 {
        v.min(10_000)
    }

    fn normalize_buyback_config(mut cfg: BuybackConfig) -> BuybackConfig {
        cfg.min_main_reserve_bp = Self::normalize_bp(cfg.min_main_reserve_bp);
        cfg.burn_share_bp = Self::normalize_bp(cfg.burn_share_bp);
        cfg.trigger_discount_bp = Self::normalize_bp(cfg.trigger_discount_bp);
        cfg.observed_market_discount_bp = Self::normalize_bp(cfg.observed_market_discount_bp);
        cfg
    }

    fn quote_multiplier_for_source(source: &str, currency: &str) -> u128 {
        let source = source.to_ascii_lowercase();
        let ccy = currency.to_ascii_uppercase();
        match (source.as_str(), ccy.as_str()) {
            ("deterministic_quote_v2", "BTC") => 90_000,
            ("deterministic_quote_v2", "ETH") => 4_000,
            ("deterministic_quote_v2", "USDT") => 1,
            (_, "BTC") => 100_000,
            (_, "ETH") => 5_000,
            (_, "USDT") => 1,
            _ => 1,
        }
    }

    fn reserve_pool_for_currency(currency: &str) -> &'static str {
        match currency.to_ascii_uppercase().as_str() {
            "USDT" => "Main",
            "BTC" | "ETH" => "RiskReserve",
            _ => "Ecosystem",
        }
    }

    /// 创建新实例
    pub fn new(config: TreasuryConfig) -> Self {
        let mut balances = HashMap::new();
        let mut total_income = HashMap::new();

        for (kind, amount) in config.initial_balances {
            balances.insert(kind, amount);
            total_income.insert(kind, amount);
        }

        Self {
            balances,
            controller: config.controller,
            nav_config: config.nav_config,
            buyback_config: Self::normalize_buyback_config(config.buyback_config),
            total_income,
            total_spent: HashMap::new(),
            foreign_reserves: HashMap::new(),
            total_miner_paid: 0,
        }
    }

    /// 检查控制者权限
    #[allow(dead_code)]
    fn require_controller(&self, caller: &Address) -> Result<()> {
        if *caller != self.controller {
            bail!("Caller is not the treasury controller");
        }
        Ok(())
    }

    /// 计算当前 NAV
    ///
    /// 实际 NAV 计算公式 (双轨定价文档):
    /// NAV = (储备资产总价值 + 主国库余额 × Token 价格) / M1 流通量
    ///
    /// 当前实现采用可复验的确定性估值:
    /// - 国库账户余额按 1:1 纳入价值
    /// - 外汇储备按内置权重折算稳定价值
    /// - 结果对齐 `min_nav_multiplier_bp` 软下限
    pub fn calculate_nav(&self) -> u128 {
        let main_balance = self
            .balances
            .get(&TreasuryAccountKind::Main)
            .copied()
            .unwrap_or(0);
        let ecosystem_balance = self
            .balances
            .get(&TreasuryAccountKind::Ecosystem)
            .copied()
            .unwrap_or(0);
        let risk_balance = self
            .balances
            .get(&TreasuryAccountKind::RiskReserve)
            .copied()
            .unwrap_or(0);
        let treasury_value = main_balance
            .saturating_add(ecosystem_balance)
            .saturating_add(risk_balance);
        let foreign_value = self
            .foreign_reserves
            .iter()
            .fold(0u128, |acc, (ccy, amount)| {
                let quoted = amount.saturating_mul(Self::quote_multiplier_for_source(
                    &self.nav_config.reserve_valuation_source,
                    ccy,
                ));
                acc.saturating_add(quoted)
            });
        let multiplier = self.nav_config.min_nav_multiplier_bp as u128;
        let floor = main_balance.saturating_mul(multiplier) / 10_000;
        treasury_value.saturating_add(foreign_value).max(floor)
    }

    /// 更新回购配置（由上游治理策略同步）
    pub fn set_buyback_config(&mut self, config: BuybackConfig) {
        self.buyback_config = Self::normalize_buyback_config(config);
    }

    /// 更新观测折价（由上游风控/执行层输入）
    pub fn set_observed_market_discount_bp(&mut self, discount_bp: u16) {
        self.buyback_config.observed_market_discount_bp = Self::normalize_bp(discount_bp);
    }

    /// 记录外汇收入
    pub fn collect_foreign_currency(
        &mut self,
        currency: String,
        amount: u128,
    ) -> Result<TreasuryEvent> {
        if amount == 0 {
            bail!("Amount must be > 0");
        }

        let entry = self.foreign_reserves.entry(currency.clone()).or_insert(0);
        *entry = entry.saturating_add(amount);

        Ok(TreasuryEvent::ForeignCurrencyCollected {
            reserve_pool: Self::reserve_pool_for_currency(&currency).to_string(),
            currency,
            amount,
        })
    }

    /// 记录矿工 Token 支付
    pub fn record_miner_payment(
        &mut self,
        miner: Address,
        token_amount: u128,
        equivalent_foreign: u128,
        foreign_currency: String,
    ) -> Result<TreasuryEvent> {
        if token_amount == 0 {
            bail!("Token amount must be > 0");
        }

        self.total_miner_paid = self.total_miner_paid.saturating_add(token_amount);

        Ok(TreasuryEvent::MinerPaidInToken {
            miner,
            token_amount,
            equivalent_foreign,
            foreign_currency,
        })
    }

    /// 查询外汇储备
    pub fn foreign_reserve(&self, currency: &str) -> u128 {
        self.foreign_reserves.get(currency).copied().unwrap_or(0)
    }

    /// 查询已支付矿工总量
    pub fn total_miner_paid(&self) -> u128 {
        self.total_miner_paid
    }
}

impl Treasury for TreasuryImpl {
    fn balance_of(&self, kind: TreasuryAccountKind) -> u128 {
        self.balances.get(&kind).copied().unwrap_or(0)
    }

    fn on_income(
        &mut self,
        from: &Address,
        amount: u128,
        kind: TreasuryAccountKind,
    ) -> Result<TreasuryEvent> {
        if amount == 0 {
            return Ok(TreasuryEvent::Income {
                from: *from,
                amount: 0,
                account: kind,
            });
        }

        let balance = self.balances.entry(kind).or_insert(0);
        *balance = balance.saturating_add(amount);

        let total = self.total_income.entry(kind).or_insert(0);
        *total = total.saturating_add(amount);

        Ok(TreasuryEvent::Income {
            from: *from,
            amount,
            account: kind,
        })
    }

    fn spend(
        &mut self,
        to: &Address,
        amount: u128,
        kind: TreasuryAccountKind,
        reason: &str,
    ) -> Result<TreasuryEvent> {
        // 注意: 实际调用时需要从 Runtime 传入调用者地址并验证权限
        // 这里暂时跳过调用者检查,仅做余额验证
        // self.require_controller(&caller)?;

        if amount == 0 {
            bail!("Spend amount must be > 0");
        }

        let balance = self.balances.get(&kind).copied().unwrap_or(0);
        if balance < amount {
            bail!(
                "Insufficient treasury balance: has {}, need {}",
                balance,
                amount
            );
        }

        self.balances.insert(kind, balance - amount);

        let total = self.total_spent.entry(kind).or_insert(0);
        *total = total.saturating_add(amount);

        Ok(TreasuryEvent::Spend {
            to: *to,
            amount,
            account: kind,
            reason: reason.to_string(),
        })
    }

    fn execute_buyback_and_burn(&mut self, max_stable_to_spend: u128) -> Result<TreasuryEvent> {
        if max_stable_to_spend == 0 {
            bail!("max_stable_to_spend must be > 0");
        }

        if self.buyback_config.observed_market_discount_bp < self.buyback_config.trigger_discount_bp
        {
            return Ok(TreasuryEvent::BuybackAndBurn {
                spent_stable: 0,
                burned_token: 0,
            });
        }

        let main_balance = self.balance_of(TreasuryAccountKind::Main);
        if main_balance == 0 {
            return Ok(TreasuryEvent::BuybackAndBurn {
                spent_stable: 0,
                burned_token: 0,
            });
        }

        let min_reserve = main_balance
            .saturating_mul(u128::from(self.buyback_config.min_main_reserve_bp))
            / 10_000;
        let available = main_balance.saturating_sub(min_reserve);
        let spent = available.min(max_stable_to_spend);
        if spent == 0 {
            return Ok(TreasuryEvent::BuybackAndBurn {
                spent_stable: 0,
                burned_token: 0,
            });
        }

        self.balances.insert(
            TreasuryAccountKind::Main,
            main_balance.saturating_sub(spent),
        );
        let total = self
            .total_spent
            .entry(TreasuryAccountKind::Main)
            .or_insert(0);
        *total = total.saturating_add(spent);

        let burned_token =
            spent.saturating_mul(u128::from(self.buyback_config.burn_share_bp)) / 10_000;
        Ok(TreasuryEvent::BuybackAndBurn {
            spent_stable: spent,
            burned_token,
        })
    }

    fn controller(&self) -> Address {
        self.controller
    }

    fn set_controller(&mut self, controller: Address) -> Result<()> {
        // 实际部署时需要验证调用者是当前 controller 或治理合约
        self.controller = controller;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(id: u8) -> Address {
        Address::from_bytes([id; 32])
    }

    fn build_treasury() -> TreasuryImpl {
        let mut initial_balances = HashMap::new();
        initial_balances.insert(TreasuryAccountKind::Main, 10_000_000);
        initial_balances.insert(TreasuryAccountKind::Ecosystem, 5_000_000);
        initial_balances.insert(TreasuryAccountKind::RiskReserve, 2_000_000);

        TreasuryImpl::new(TreasuryConfig {
            initial_balances,
            controller: addr(100),
            buyback_config: BuybackConfig::default(),
            nav_config: NavConfig::default(),
        })
    }

    #[test]
    fn test_treasury_balances() {
        let treasury = build_treasury();
        assert_eq!(treasury.balance_of(TreasuryAccountKind::Main), 10_000_000);
        assert_eq!(
            treasury.balance_of(TreasuryAccountKind::Ecosystem),
            5_000_000
        );
        assert_eq!(
            treasury.balance_of(TreasuryAccountKind::RiskReserve),
            2_000_000
        );
    }

    #[test]
    fn test_income_routing() {
        let mut treasury = build_treasury();
        let from = addr(1);

        let event = treasury
            .on_income(&from, 1_000_000, TreasuryAccountKind::Main)
            .expect("income");

        if let TreasuryEvent::Income { amount, .. } = event {
            assert_eq!(amount, 1_000_000);
        } else {
            panic!("unexpected event");
        }

        assert_eq!(treasury.balance_of(TreasuryAccountKind::Main), 11_000_000);
    }

    #[test]
    fn test_spend() {
        let mut treasury = build_treasury();
        let to = addr(2);

        let event = treasury
            .spend(&to, 500_000, TreasuryAccountKind::Ecosystem, "Grant")
            .expect("spend");

        if let TreasuryEvent::Spend { amount, reason, .. } = event {
            assert_eq!(amount, 500_000);
            assert_eq!(reason, "Grant");
        } else {
            panic!("unexpected event");
        }

        assert_eq!(
            treasury.balance_of(TreasuryAccountKind::Ecosystem),
            4_500_000
        );
    }

    #[test]
    fn test_insufficient_balance() {
        let mut treasury = build_treasury();
        let to = addr(3);

        let result = treasury.spend(&to, 20_000_000, TreasuryAccountKind::Main, "Too much");
        assert!(result.is_err());
    }

    #[test]
    fn test_foreign_currency_collection() {
        let mut treasury = build_treasury();

        let event = treasury
            .collect_foreign_currency("BTC".into(), 100_000_000)
            .expect("collect");

        if let TreasuryEvent::ForeignCurrencyCollected {
            currency, amount, ..
        } = event
        {
            assert_eq!(currency, "BTC");
            assert_eq!(amount, 100_000_000);
        } else {
            panic!("unexpected event");
        }

        assert_eq!(treasury.foreign_reserve("BTC"), 100_000_000);
        assert_eq!(treasury.foreign_reserve("ETH"), 0);
    }

    #[test]
    fn test_miner_payment_record() {
        let mut treasury = build_treasury();
        let miner = addr(50);

        let event = treasury
            .record_miner_payment(miner, 5_000_000, 100_000, "USDT".into())
            .expect("miner payment");

        if let TreasuryEvent::MinerPaidInToken {
            token_amount,
            equivalent_foreign,
            foreign_currency,
            ..
        } = event
        {
            assert_eq!(token_amount, 5_000_000);
            assert_eq!(equivalent_foreign, 100_000);
            assert_eq!(foreign_currency, "USDT");
        } else {
            panic!("unexpected event");
        }

        assert_eq!(treasury.total_miner_paid(), 5_000_000);
    }

    #[test]
    fn test_nav_calculation() {
        let treasury = build_treasury();
        let nav = treasury.calculate_nav();
        // treasury_value = 10_000_000 + 5_000_000 + 2_000_000
        assert_eq!(nav, 17_000_000);
    }

    #[test]
    fn test_nav_includes_foreign_reserve_valuation() {
        let mut treasury = build_treasury();
        treasury
            .collect_foreign_currency("BTC".into(), 10)
            .expect("collect btc");
        treasury
            .collect_foreign_currency("ETH".into(), 20)
            .expect("collect eth");
        treasury
            .collect_foreign_currency("USDT".into(), 30)
            .expect("collect usdt");
        let nav = treasury.calculate_nav();
        // base treasury = 17_000_000
        // foreign = 10*100_000 + 20*5_000 + 30 = 1_100_030
        assert_eq!(nav, 18_100_030);
    }

    #[test]
    fn test_buyback_zero_budget_reject() {
        let mut treasury = build_treasury();
        let err = treasury
            .execute_buyback_and_burn(0)
            .expect_err("zero budget must be rejected");
        assert!(err.to_string().contains("max_stable_to_spend must be > 0"));
    }

    #[test]
    fn test_buyback_not_triggered_when_discount_below_threshold() {
        let mut treasury = build_treasury();
        treasury.set_buyback_config(BuybackConfig {
            min_main_reserve_bp: 1_000,
            burn_share_bp: 5_000,
            trigger_discount_bp: 800,
            observed_market_discount_bp: 700,
        });

        let event = treasury
            .execute_buyback_and_burn(1_000_000)
            .expect("buyback execution");
        match event {
            TreasuryEvent::BuybackAndBurn {
                spent_stable,
                burned_token,
            } => {
                assert_eq!(spent_stable, 0);
                assert_eq!(burned_token, 0);
            }
            other => panic!("unexpected event: {:?}", other),
        }
        assert_eq!(treasury.balance_of(TreasuryAccountKind::Main), 10_000_000);
    }

    #[test]
    fn test_buyback_respects_min_main_reserve_and_burn_share() {
        let mut treasury = build_treasury();
        treasury.set_buyback_config(BuybackConfig {
            min_main_reserve_bp: 2_500, // keep 25%
            burn_share_bp: 6_000,       // burn 60%
            trigger_discount_bp: 500,
            observed_market_discount_bp: 650,
        });

        let event = treasury
            .execute_buyback_and_burn(9_000_000)
            .expect("buyback execution");
        match event {
            TreasuryEvent::BuybackAndBurn {
                spent_stable,
                burned_token,
            } => {
                // initial main=10_000_000, reserve=2_500_000, available=7_500_000
                assert_eq!(spent_stable, 7_500_000);
                assert_eq!(burned_token, 4_500_000);
            }
            other => panic!("unexpected event: {:?}", other),
        }
        assert_eq!(treasury.balance_of(TreasuryAccountKind::Main), 2_500_000);
    }

    #[test]
    fn test_controller_management() {
        let mut treasury = build_treasury();
        assert_eq!(treasury.controller(), addr(100));

        treasury.set_controller(addr(101)).expect("set controller");
        assert_eq!(treasury.controller(), addr(101));
    }
}
