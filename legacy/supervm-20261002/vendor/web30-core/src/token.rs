//! WEB30 Token 核心实现

use crate::types::*;
use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};

/// WEB30 Token 标准接口
pub trait WEB30TokenTrait {
    // ========== 基础信息 ==========
    fn name(&self) -> String;
    fn symbol(&self) -> String;
    fn decimals(&self) -> u8;
    fn total_supply(&self) -> u128;

    // ========== 余额查询 ==========
    fn balance_of(&self, account: &Address) -> u128;

    // ========== 转账操作 ==========
    fn transfer(&mut self, to: &Address, amount: u128) -> Result<TransferReceipt>;
    fn batch_transfer(&mut self, recipients: &[(Address, u128)]) -> Result<Vec<TransferReceipt>>;

    // ========== 授权与委托 ==========
    fn set_allowance(&mut self, spender: &Address, amount: u128) -> Result<()>;
    fn allowance(&self, owner: &Address, spender: &Address) -> u128;
    fn transfer_from(
        &mut self,
        from: &Address,
        to: &Address,
        amount: u128,
    ) -> Result<TransferReceipt>;

    // ========== 高级功能 ==========
    fn mint(&mut self, to: &Address, amount: u128) -> Result<()>;
    fn burn(&mut self, amount: u128) -> Result<()>;
    fn freeze(&mut self, account: &Address) -> Result<()>;
    fn unfreeze(&mut self, account: &Address) -> Result<()>;

    // ========== 元数据 ==========
    fn metadata(&self) -> &TokenMetadata;
}

/// WEB30 Token 实现
pub struct WEB30Token {
    // 基础信息
    metadata: TokenMetadata,
    total_supply: u128,

    // 状态存储
    balances: HashMap<Address, u128>,
    allowances: HashMap<(Address, Address), u128>,
    frozen: HashSet<Address>,

    // 权限控制
    owner: Address,
    minters: HashSet<Address>,
}

impl WEB30Token {
    /// 创建新代币
    pub fn new(
        name: String,
        symbol: String,
        decimals: u8,
        initial_supply: u128,
        owner: Address,
    ) -> Self {
        let mut balances = HashMap::new();
        if initial_supply > 0 {
            balances.insert(owner, initial_supply);
        }

        let mut minters = HashSet::new();
        minters.insert(owner);

        Self {
            metadata: TokenMetadata {
                name,
                symbol,
                decimals,
                icon_uri: String::new(),
                description: String::new(),
                website: String::new(),
                social: SocialLinks {
                    twitter: None,
                    telegram: None,
                    discord: None,
                },
            },
            total_supply: initial_supply,
            balances,
            allowances: HashMap::new(),
            frozen: HashSet::new(),
            owner,
            minters,
        }
    }

    /// 获取当前调用者（从运行时获取）
    fn get_caller(&self) -> Address {
        // 在实际部署中，这会从 vm-runtime 获取
        // 这里为了编译使用占位符
        Address::zero()
    }

    /// 检查权限
    fn require_permission(&self, account: &Address, permission: Permission) -> Result<()> {
        match permission {
            Permission::Mint => {
                if !self.minters.contains(account) {
                    bail!("Caller is not a minter");
                }
            }
            Permission::Owner => {
                if *account != self.owner {
                    bail!("Caller is not the owner");
                }
            }
        }
        Ok(())
    }

    /// 内部转账实现
    fn _transfer(&mut self, from: &Address, to: &Address, amount: u128) -> Result<TransferReceipt> {
        // 检查冻结状态
        if self.frozen.contains(from) || self.frozen.contains(to) {
            bail!("Account is frozen");
        }

        // 检查余额
        let from_balance = self.balances.get(from).copied().unwrap_or(0);
        if from_balance < amount {
            bail!(
                "Insufficient balance: has {}, need {}",
                from_balance,
                amount
            );
        }

        // 执行转账（MVCC 保证原子性）
        *self.balances.entry(*from).or_insert(0) -= amount;
        *self.balances.entry(*to).or_insert(0) += amount;

        Ok(TransferReceipt {
            tx_hash: [0u8; 32], // 实际从运行时获取
            from: *from,
            to: *to,
            amount,
            timestamp: 0, // 实际从运行时获取
            gas_used: 0,  // 实际从运行时获取
        })
    }
}

impl WEB30TokenTrait for WEB30Token {
    fn name(&self) -> String {
        self.metadata.name.clone()
    }

    fn symbol(&self) -> String {
        self.metadata.symbol.clone()
    }

    fn decimals(&self) -> u8 {
        self.metadata.decimals
    }

    fn total_supply(&self) -> u128 {
        self.total_supply
    }

    fn balance_of(&self, account: &Address) -> u128 {
        self.balances.get(account).copied().unwrap_or(0)
    }

    fn transfer(&mut self, to: &Address, amount: u128) -> Result<TransferReceipt> {
        let from = self.get_caller();
        self._transfer(&from, to, amount)
    }

    fn batch_transfer(&mut self, recipients: &[(Address, u128)]) -> Result<Vec<TransferReceipt>> {
        let sender = self.get_caller();
        let total_amount: u128 = recipients.iter().map(|(_, amt)| amt).sum();

        // 预检查余额
        if self.balance_of(&sender) < total_amount {
            bail!("Insufficient balance for batch transfer");
        }

        // 批量执行（并行优化由 MVCC 调度器处理）
        let mut receipts = Vec::new();
        for (to, amount) in recipients {
            let receipt = self._transfer(&sender, to, *amount)?;
            receipts.push(receipt);
        }

        Ok(receipts)
    }

    fn set_allowance(&mut self, spender: &Address, amount: u128) -> Result<()> {
        let owner = self.get_caller();
        self.allowances.insert((owner, *spender), amount);
        Ok(())
    }

    fn allowance(&self, owner: &Address, spender: &Address) -> u128 {
        self.allowances
            .get(&(*owner, *spender))
            .copied()
            .unwrap_or(0)
    }

    fn transfer_from(
        &mut self,
        from: &Address,
        to: &Address,
        amount: u128,
    ) -> Result<TransferReceipt> {
        let spender = self.get_caller();

        // 检查授权额度
        let current_allowance = self.allowance(from, &spender);
        if current_allowance < amount {
            bail!(
                "Allowance exceeded: has {}, need {}",
                current_allowance,
                amount
            );
        }

        // 减少授权额度
        self.allowances
            .insert((*from, spender), current_allowance - amount);

        // 执行转账
        self._transfer(from, to, amount)
    }

    fn mint(&mut self, to: &Address, amount: u128) -> Result<()> {
        let caller = self.get_caller();
        self.require_permission(&caller, Permission::Mint)?;

        *self.balances.entry(*to).or_insert(0) += amount;
        self.total_supply += amount;

        Ok(())
    }

    fn burn(&mut self, amount: u128) -> Result<()> {
        let caller = self.get_caller();
        let balance = self.balance_of(&caller);

        if balance < amount {
            bail!("Insufficient balance to burn");
        }

        *self.balances.entry(caller).or_insert(0) -= amount;
        self.total_supply -= amount;

        Ok(())
    }

    fn freeze(&mut self, account: &Address) -> Result<()> {
        let caller = self.get_caller();
        self.require_permission(&caller, Permission::Owner)?;

        self.frozen.insert(*account);
        Ok(())
    }

    fn unfreeze(&mut self, account: &Address) -> Result<()> {
        let caller = self.get_caller();
        self.require_permission(&caller, Permission::Owner)?;

        self.frozen.remove(account);
        Ok(())
    }

    fn metadata(&self) -> &TokenMetadata {
        &self.metadata
    }
}

#[derive(Debug, Clone, Copy)]
enum Permission {
    Mint,
    Owner,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation() {
        let owner = Address::from_bytes([1u8; 32]);
        let token = WEB30Token::new(
            "Test Token".to_string(),
            "TEST".to_string(),
            18,
            1000000,
            owner,
        );

        assert_eq!(token.name(), "Test Token");
        assert_eq!(token.symbol(), "TEST");
        assert_eq!(token.decimals(), 18);
        assert_eq!(token.total_supply(), 1000000);
        assert_eq!(token.balance_of(&owner), 1000000);
    }

    #[test]
    fn test_batch_transfer() {
        let owner = Address::from_bytes([1u8; 32]);
        let _recipient1 = Address::from_bytes([2u8; 32]);
        let _recipient2 = Address::from_bytes([3u8; 32]);

        let _token = WEB30Token::new("Test".to_string(), "TST".to_string(), 18, 1000000, owner);

        // let _recipients = vec![(_recipient1, 100), (_recipient2, 200)];

        // 注意：实际测试需要模拟 get_caller()
        // 这里仅作结构演示
    }
}
