//! SuperVM MainnetToken 参考实现
//!
//! 该实现依托 `MainnetToken` trait, 面向 WASM Runtime 直接调用。
//! - 负责基本余额管理、解锁/销毁逻辑
//! - 在 Gas/服务费入口完成拆分与路由, 并生成事件供执行层记录
//! - 预留治理可调的参数 (FeeSplit / Unlock Controller)

use std::collections::HashMap;

use anyhow::{bail, Result};

use crate::{
    mainnet_token::{FeeSplit, MainnetToken, MainnetTokenEvent},
    types::Address,
};

/// MainnetToken 初始化参数
pub struct MainnetTokenConfig {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub max_supply: u128,
    pub initial_allocations: Vec<(Address, u128)>,
    pub locked_supply: u128,
    pub fee_split: FeeSplit,
    pub treasury_account: Address,
    pub node_reward_pool: Address,
    pub service_provider_pool: Address,
    pub unlock_controller: Address,
}

impl MainnetTokenConfig {
    fn validate(&self) -> Result<()> {
        ensure_basis_points(self.fee_split)?;

        let circulating: u128 = self
            .initial_allocations
            .iter()
            .map(|(_, amount)| *amount)
            .sum();

        if circulating + self.locked_supply > self.max_supply {
            bail!(
                "Initial circulating ({circulating}) + locked ({}) exceeds max supply ({})",
                self.locked_supply,
                self.max_supply
            );
        }

        Ok(())
    }
}

/// MainnetToken 具体实现
pub struct MainnetTokenImpl {
    name: String,
    symbol: String,
    decimals: u8,
    total_supply: u128,
    max_supply: u128,
    locked_supply: u128,
    balances: HashMap<Address, u128>,
    fee_split: FeeSplit,
    treasury_account: Address,
    node_reward_pool: Address,
    service_provider_pool: Address,
    unlock_controller: Address,
}

impl MainnetTokenImpl {
    /// 获取国库账户地址
    pub fn treasury_account(&self) -> Address {
        self.treasury_account
    }

    /// 创建实例
    pub fn new(config: MainnetTokenConfig) -> Result<Self> {
        config.validate()?;

        let mut balances = HashMap::new();
        let mut total_supply = 0u128;
        for (addr, amount) in config.initial_allocations.iter() {
            if *amount == 0 {
                continue;
            }
            balances.insert(*addr, *amount);
            total_supply = total_supply.saturating_add(*amount);
        }

        Ok(Self {
            name: config.name,
            symbol: config.symbol,
            decimals: config.decimals,
            total_supply,
            max_supply: config.max_supply,
            locked_supply: config.locked_supply,
            balances,
            fee_split: config.fee_split,
            treasury_account: config.treasury_account,
            node_reward_pool: config.node_reward_pool,
            service_provider_pool: config.service_provider_pool,
            unlock_controller: config.unlock_controller,
        })
    }

    fn debit(&mut self, owner: &Address, amount: u128) -> Result<()> {
        let balance = self.balances.get(owner).copied().unwrap_or(0);
        if balance < amount {
            bail!("Insufficient balance: has {}, need {}", balance, amount);
        }
        if amount == 0 {
            return Ok(());
        }
        self.balances.insert(*owner, balance - amount);
        Ok(())
    }

    fn credit(&mut self, owner: &Address, amount: u128) {
        if amount == 0 {
            return;
        }
        let entry = self.balances.entry(*owner).or_insert(0);
        *entry = entry.saturating_add(amount);
    }

    fn burn_amount(&mut self, amount: u128) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }
        if amount > self.total_supply {
            bail!(
                "Burn amount {} exceeds total supply {}",
                amount,
                self.total_supply
            );
        }
        self.total_supply -= amount;
        Ok(())
    }

    fn perform_transfer(&mut self, from: &Address, to: &Address, amount: u128) -> Result<()> {
        if amount == 0 {
            return Ok(());
        }
        self.debit(from, amount)?;
        self.credit(to, amount);
        Ok(())
    }

    fn apply_split(&self, amount: u128, to_actor_bp: u16, burn_bp: u16) -> (u128, u128, u128) {
        let amount_u128 = amount;
        let to_actor = amount_u128 * to_actor_bp as u128 / 10_000u128;
        let to_burn = amount_u128 * burn_bp as u128 / 10_000u128;
        let to_treasury = amount_u128.saturating_sub(to_actor + to_burn);
        (to_actor, to_burn, to_treasury)
    }
}

impl MainnetToken for MainnetTokenImpl {
    fn name(&self) -> &str {
        &self.name
    }

    fn symbol(&self) -> &str {
        &self.symbol
    }

    fn decimals(&self) -> u8 {
        self.decimals
    }

    fn total_supply(&self) -> u128 {
        self.total_supply
    }

    fn max_supply(&self) -> u128 {
        self.max_supply
    }

    fn circulating_supply(&self) -> u128 {
        self.total_supply
    }

    fn locked_supply(&self) -> u128 {
        self.locked_supply
    }

    fn balance_of(&self, owner: &Address) -> u128 {
        self.balances.get(owner).copied().unwrap_or(0)
    }

    fn transfer(&mut self, from: &Address, to: &Address, amount: u128) -> Result<()> {
        self.perform_transfer(from, to, amount)
    }

    fn mint(&mut self, to: &Address, amount: u128) -> Result<MainnetTokenEvent> {
        if amount == 0 {
            bail!("Mint amount must be > 0");
        }
        if amount > self.locked_supply {
            bail!(
                "Mint amount {} exceeds locked supply {}",
                amount,
                self.locked_supply
            );
        }
        if self.total_supply + amount > self.max_supply {
            bail!("Mint would exceed max supply");
        }

        self.locked_supply -= amount;
        self.total_supply += amount;
        self.credit(to, amount);

        Ok(MainnetTokenEvent::Mint { to: *to, amount })
    }

    fn burn(&mut self, from: &Address, amount: u128) -> Result<MainnetTokenEvent> {
        self.debit(from, amount)?;
        self.burn_amount(amount)?;
        Ok(MainnetTokenEvent::Burn {
            from: *from,
            amount,
        })
    }

    fn on_gas_fee_paid(&mut self, payer: &Address, amount: u128) -> Result<MainnetTokenEvent> {
        self.debit(payer, amount)?;
        let (to_node, to_burn, to_treasury) = self.apply_split(
            amount,
            self.fee_split.gas_to_node_bp,
            self.fee_split.gas_base_burn_bp,
        );

        let node_account = self.node_reward_pool;
        let treasury_account = self.treasury_account;

        self.credit(&node_account, to_node);
        self.credit(&treasury_account, to_treasury);
        self.burn_amount(to_burn)?;

        Ok(MainnetTokenEvent::GasFeeRouted {
            payer: *payer,
            amount,
            to_node,
            to_treasury,
            to_burn,
        })
    }

    fn on_service_fee_paid(
        &mut self,
        service_id: [u8; 32],
        payer: &Address,
        amount: u128,
    ) -> Result<MainnetTokenEvent> {
        self.debit(payer, amount)?;
        let (to_provider, to_burn, to_treasury) = self.apply_split(
            amount,
            self.fee_split.service_to_provider_bp,
            self.fee_split.service_burn_bp,
        );

        let provider_pool = self.service_provider_pool;
        let treasury_account = self.treasury_account;

        self.credit(&provider_pool, to_provider);
        self.credit(&treasury_account, to_treasury);
        self.burn_amount(to_burn)?;

        Ok(MainnetTokenEvent::ServiceFeeRouted {
            service_id,
            payer: *payer,
            amount,
            to_provider,
            to_treasury,
            to_burn,
        })
    }

    fn fee_split(&self) -> FeeSplit {
        self.fee_split
    }

    fn set_fee_split(&mut self, split: FeeSplit) -> Result<()> {
        ensure_basis_points(split)?;
        self.fee_split = split;
        Ok(())
    }

    fn unlock_controller(&self) -> Address {
        self.unlock_controller
    }

    fn set_unlock_controller(&mut self, controller: Address) -> Result<()> {
        self.unlock_controller = controller;
        Ok(())
    }
}

fn ensure_basis_points(split: FeeSplit) -> Result<()> {
    let gas_total = split.gas_base_burn_bp as u32 + split.gas_to_node_bp as u32;
    if gas_total > 10_000 {
        bail!("Gas fee split exceeds 100%: {} bp", gas_total);
    }

    let service_total = split.service_burn_bp as u32 + split.service_to_provider_bp as u32;
    if service_total > 10_000 {
        bail!("Service fee split exceeds 100%: {} bp", service_total);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(id: u8) -> Address {
        Address::from_bytes([id; 32])
    }

    fn default_split() -> FeeSplit {
        FeeSplit {
            gas_base_burn_bp: 2000,
            gas_to_node_bp: 3000,
            service_burn_bp: 1000,
            service_to_provider_bp: 4000,
        }
    }

    fn build_token() -> MainnetTokenImpl {
        MainnetTokenImpl::new(MainnetTokenConfig {
            name: "SuperVM".into(),
            symbol: "SVM".into(),
            decimals: 9,
            max_supply: 1_000_000_000_000,
            initial_allocations: vec![(addr(1), 1_000_000_000)],
            locked_supply: 100_000_000_000,
            fee_split: default_split(),
            treasury_account: addr(200),
            node_reward_pool: addr(201),
            service_provider_pool: addr(202),
            unlock_controller: addr(250),
        })
        .expect("init")
    }

    #[test]
    fn test_transfer_and_balance() {
        let mut token = build_token();
        let from = addr(1);
        let to = addr(2);
        token.transfer(&from, &to, 500).expect("transfer");
        assert_eq!(token.balance_of(&from), 1_000_000_000 - 500);
        assert_eq!(token.balance_of(&to), 500);
    }

    #[test]
    fn test_mint_and_burn() {
        let mut token = build_token();
        let target = addr(3);
        let event = token.mint(&target, 1_000).expect("mint");
        if let MainnetTokenEvent::Mint { amount, .. } = event {
            assert_eq!(amount, 1_000);
        } else {
            panic!("unexpected event");
        }
        assert_eq!(token.balance_of(&target), 1_000);

        let burn_event = token.burn(&target, 400).expect("burn");
        if let MainnetTokenEvent::Burn { amount, .. } = burn_event {
            assert_eq!(amount, 400);
        } else {
            panic!("unexpected burn event");
        }
        assert_eq!(token.balance_of(&target), 600);
    }

    #[test]
    fn test_gas_fee_routing() {
        let mut token = build_token();
        let payer = addr(1);
        let event = token.on_gas_fee_paid(&payer, 1_000_000).expect("gas fee");

        if let MainnetTokenEvent::GasFeeRouted {
            to_node,
            to_treasury,
            to_burn,
            ..
        } = event
        {
            assert_eq!(to_node, 300_000);
            assert_eq!(to_burn, 200_000);
            assert_eq!(to_treasury, 500_000);
        } else {
            panic!("unexpected gas event");
        }

        assert_eq!(token.balance_of(&addr(201)), 300_000);
        assert_eq!(token.balance_of(&addr(200)), 500_000);
    }

    #[test]
    fn test_service_fee_routing() {
        let mut token = build_token();
        let payer = addr(1);
        let event = token
            .on_service_fee_paid([1u8; 32], &payer, 2_000_000)
            .expect("service fee");

        if let MainnetTokenEvent::ServiceFeeRouted {
            to_provider,
            to_treasury,
            to_burn,
            ..
        } = event
        {
            assert_eq!(to_provider, 800_000);
            assert_eq!(to_burn, 200_000);
            assert_eq!(to_treasury, 1_000_000);
        } else {
            panic!("unexpected service event");
        }

        assert_eq!(token.balance_of(&addr(202)), 800_000);
        assert_eq!(token.balance_of(&addr(200)), 1_000_000);
    }
}
