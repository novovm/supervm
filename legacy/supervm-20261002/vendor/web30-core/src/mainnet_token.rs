//! SuperVM 主链 Token 原生规范 (Rust/WASM)
//!
//! 对应经济模型: M0/M1/M2 + Gas/服务费销毁 + 费用路由。
//! Solidity 版 `MainnetToken.sol` 仅作为 EVM 兼容层/镜像资产使用,
//! 真实主网执行逻辑以本 Rust/WASM 规范为准。

use crate::types::Address;
use anyhow::Result;

/// 费用拆分参数 (单位: 基点, 10000 = 100%)
#[derive(Debug, Clone, Copy)]
pub struct FeeSplit {
    pub gas_base_burn_bp: u16,
    pub gas_to_node_bp: u16,
    pub service_burn_bp: u16,
    pub service_to_provider_bp: u16,
}

/// 主链 Token 关键事件的抽象,方便在 VM 中记录日志/指标。
#[derive(Debug, Clone)]
pub enum MainnetTokenEvent {
    Mint {
        to: Address,
        amount: u128,
    },
    Burn {
        from: Address,
        amount: u128,
    },
    GasFeeRouted {
        payer: Address,
        amount: u128,
        to_node: u128,
        to_treasury: u128,
        to_burn: u128,
    },
    ServiceFeeRouted {
        service_id: [u8; 32],
        payer: Address,
        amount: u128,
        to_provider: u128,
        to_treasury: u128,
        to_burn: u128,
    },
}

/// SuperVM 主链 Token 原生接口
pub trait MainnetToken {
    // -------- Meta --------
    fn name(&self) -> &str;
    fn symbol(&self) -> &str;
    fn decimals(&self) -> u8;

    // -------- Supply & Caps --------
    fn total_supply(&self) -> u128;
    fn max_supply(&self) -> u128;

    /// 流通供应(M1, 可转移部分)
    fn circulating_supply(&self) -> u128;

    /// 锁定供应(尚未解锁的 M0 部分)
    fn locked_supply(&self) -> u128;

    // -------- Balance & Transfer --------
    fn balance_of(&self, owner: &Address) -> u128;

    fn transfer(&mut self, from: &Address, to: &Address, amount: u128) -> Result<()>;

    // -------- Mint / Burn (由解锁/国库策略控制) --------

    /// 从 M0 池铸造新 Token 到指定地址,仅解锁控制逻辑可调用。
    fn mint(&mut self, to: &Address, amount: u128) -> Result<MainnetTokenEvent>;

    /// 主动销毁调用者余额中的 Token。
    fn burn(&mut self, from: &Address, amount: u128) -> Result<MainnetTokenEvent>;

    // -------- Fee Routing Hooks --------

    /// Gas 费用支付入口: 由执行层在交易结算时调用。
    /// 实现内部分拆为: 节点奖励/国库/销毁,并返回事件用于记录。
    fn on_gas_fee_paid(&mut self, payer: &Address, amount: u128) -> Result<MainnetTokenEvent>;

    /// 服务费用支付入口: 由上层服务网关调用。
    fn on_service_fee_paid(
        &mut self,
        service_id: [u8; 32],
        payer: &Address,
        amount: u128,
    ) -> Result<MainnetTokenEvent>;

    // -------- Governance / Parameters --------

    fn fee_split(&self) -> FeeSplit;

    fn set_fee_split(&mut self, split: FeeSplit) -> Result<()>;

    /// 可选: 返回当前解锁控制地址/标识,便于在协议里接管货币政策。
    fn unlock_controller(&self) -> Address;

    fn set_unlock_controller(&mut self, controller: Address) -> Result<()>;
}
