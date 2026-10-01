//! SuperVM 协议国库(Treasury) 原生规范
//!
//! 设计原则:
//! - 收入侧(费用→国库/销毁)100% 自动,由协议/VM 调用,不可被单人篡改;
//! - 支出侧(投资/补贴/回购)由治理控制,在预定义的策略合约/接口之内执行;
//! - 与 `MainnetToken` 配合,形成完整的经济闭环。

use crate::types::Address;
use anyhow::Result;

/// 国库账户类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreasuryAccountKind {
    /// 主国库: 协议公共资金池
    Main,
    /// 生态基金: Grants/黑客松/早期 DApp 支持
    Ecosystem,
    /// 风险准备金: 清算、极端事件兜底
    RiskReserve,
}

/// 国库侧记录的事件
#[derive(Debug, Clone)]
pub enum TreasuryEvent {
    /// 收入: 来自 Gas/服务费路由,已由 Token 层拆分好
    Income {
        from: Address,
        amount: u128,
        account: TreasuryAccountKind,
    },

    /// 支出: 经治理授权后执行
    Spend {
        to: Address,
        amount: u128,
        account: TreasuryAccountKind,
        reason: String,
    },

    /// 回购+销毁操作(可作为一个策略的结果)
    BuybackAndBurn {
        spent_stable: u128,
        burned_token: u128,
    },

    /// 外汇收入归集(币种, 金额, 接收的外汇储备池)
    ForeignCurrencyCollected {
        currency: String, // "BTC", "ETH", "USDT"等
        amount: u128,
        reserve_pool: String,
    },

    /// Token替代支付给矿主(矿主地址, Token数量, 等值外币金额, 币种)
    MinerPaidInToken {
        miner: Address,
        token_amount: u128,
        equivalent_foreign: u128,
        foreign_currency: String,
    },
}

/// 国库原生接口: 自动入账 + 治理控制支出
pub trait Treasury {
    /// 查询指定国库账户余额
    fn balance_of(&self, kind: TreasuryAccountKind) -> u128;

    /// 收入侧入口: 仅由执行层/Token 层调用
    /// 例如: on_gas_fee_paid / on_service_fee_paid 拆分后将一部分记入 Main/Ecosystem/RiskReserve
    fn on_income(
        &mut self,
        from: &Address,
        amount: u128,
        kind: TreasuryAccountKind,
    ) -> Result<TreasuryEvent>;

    /// 支出侧: 仅允许经治理授权的控制者调用
    /// 典型用途: 生态 Grants、研发资助、运营支出、回购预算划转等
    fn spend(
        &mut self,
        to: &Address,
        amount: u128,
        kind: TreasuryAccountKind,
        reason: &str,
    ) -> Result<TreasuryEvent>;

    /// 可选: 一键执行“回购+销毁”策略(由上层策略模块/治理调度)
    /// - `max_stable_to_spend`: 本次最多愿意花费的稳定资产数量
    /// - 返回: 实际花费的稳定资产,以及对应销毁的主链 Token 数量
    fn execute_buyback_and_burn(&mut self, max_stable_to_spend: u128) -> Result<TreasuryEvent>;

    /// 返回当前拥有支出权限/策略执行权限的治理控制者地址(可为多签/DAO 网关)
    fn controller(&self) -> Address;

    /// 更新控制者地址(需在链上治理流程中调用)
    fn set_controller(&mut self, controller: Address) -> Result<()>;
}
