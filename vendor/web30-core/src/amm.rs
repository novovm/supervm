//! AMM module (Automated Market Maker)
//!
//! Implements constant product AMM (x·y=k) for:
//! - Liquidity provision for Token/Fiat pairs
//! - Exchange rate reference for foreign payments
//! - Miner Token swaps to foreign currency
//! - Market price inputs for NAV calculations

use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};

use crate::types::Address;

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut x0 = value;
    let mut x1 = (x0 + value / x0) / 2;
    while x1 < x0 {
        x0 = x1;
        x1 = (x0 + value / x0) / 2;
    }
    x0
}

fn checked_mul(a: u128, b: u128, ctx: &str) -> Result<u128> {
    a.checked_mul(b)
        .ok_or_else(|| anyhow!("overflow in {}", ctx))
}

fn checked_add(a: u128, b: u128, ctx: &str) -> Result<u128> {
    a.checked_add(b)
        .ok_or_else(|| anyhow!("overflow in {}", ctx))
}

fn checked_sub(a: u128, b: u128, ctx: &str) -> Result<u128> {
    a.checked_sub(b)
        .ok_or_else(|| anyhow!("underflow in {}", ctx))
}

/// Liquidity pool
#[derive(Debug, Clone)]
pub struct LiquidityPool {
    /// Pool ID
    pub pool_id: [u8; 32],
    /// Token A reserve
    pub reserve_a: u128,
    /// Token B reserve
    pub reserve_b: u128,
    /// Token A symbol
    pub token_a: String,
    /// Token B symbol
    pub token_b: String,
    /// LP token total supply
    pub lp_total_supply: u128,
    /// Fee rate (basis points, 30 = 0.3%)
    pub fee_bps: u16,
}

impl LiquidityPool {
    /// Create a new pool
    pub fn new(
        pool_id: [u8; 32],
        token_a: String,
        token_b: String,
        initial_a: u128,
        initial_b: u128,
        fee_bps: u16,
    ) -> Result<Self> {
        if initial_a == 0 || initial_b == 0 {
            bail!("Initial reserves must be > 0");
        }
        if fee_bps > 10_000 {
            bail!("Fee exceeds 100%");
        }

        // Initial LP = sqrt(x * y), integer-only to keep deterministic transitions.
        let k = checked_mul(initial_a, initial_b, "pool init k")?;
        let lp_supply = integer_sqrt(k);
        if lp_supply == 0 {
            bail!("Initial LP supply underflow");
        }

        Ok(Self {
            pool_id,
            reserve_a: initial_a,
            reserve_b: initial_b,
            token_a,
            token_b,
            lp_total_supply: lp_supply,
            fee_bps,
        })
    }

    /// Compute output amount using x·y=k
    /// Input dx, output dy
    /// (x + dx * (1 - fee)) * (y - dy) = k
    pub fn get_amount_out(
        &self,
        amount_in: u128,
        reserve_in: u128,
        reserve_out: u128,
    ) -> Result<u128> {
        if amount_in == 0 {
            bail!("Amount in must be > 0");
        }
        if reserve_in == 0 || reserve_out == 0 {
            bail!("Insufficient liquidity");
        }

        // Deduct fee
        let amount_in_with_fee = checked_mul(
            amount_in,
            10_000 - self.fee_bps as u128,
            "amount_in_with_fee numerator",
        )? / 10_000;

        // dy = y * dx / (x + dx)
        let numerator = checked_mul(amount_in_with_fee, reserve_out, "amount_out numerator")?;
        let denominator = checked_add(reserve_in, amount_in_with_fee, "amount_out denominator")?;

        Ok(numerator / denominator)
    }

    /// Compute input amount (reverse calculation)
    pub fn get_amount_in(
        &self,
        amount_out: u128,
        reserve_in: u128,
        reserve_out: u128,
    ) -> Result<u128> {
        if amount_out == 0 {
            bail!("Amount out must be > 0");
        }
        if reserve_in == 0 || reserve_out == 0 {
            bail!("Insufficient liquidity");
        }
        if amount_out >= reserve_out {
            bail!("Amount out exceeds reserve");
        }

        // dx = x * dy / ((y - dy) * (1 - fee))
        let numerator = checked_mul(
            checked_mul(reserve_in, amount_out, "amount_in numerator x*y")?,
            10_000,
            "amount_in numerator * 10000",
        )?;
        let reserve_out_after = checked_sub(reserve_out, amount_out, "amount_in reserve_out-dy")?;
        let denominator = checked_mul(
            reserve_out_after,
            10_000 - self.fee_bps as u128,
            "amount_in denominator",
        )?;

        Ok(numerator / denominator + 1) // +1 round up
    }

    /// Current price as ratio (B/A), integer deterministic.
    pub fn price_ratio(&self) -> Option<(u128, u128)> {
        if self.reserve_a == 0 {
            return None;
        }
        Some((self.reserve_b, self.reserve_a))
    }

    /// Current price scaled by caller-provided factor.
    /// Example: scale=1_000_000 => 1.0 is represented as 1_000_000.
    pub fn price_scaled(&self, scale: u128) -> Result<u128> {
        if scale == 0 {
            bail!("price scale must be > 0");
        }
        let (numerator, denominator) = self
            .price_ratio()
            .ok_or_else(|| anyhow!("price unavailable: reserve_a is zero"))?;
        let scaled_numerator = checked_mul(numerator, scale, "price scaled numerator")?;
        Ok(scaled_numerator / denominator)
    }

    /// Add liquidity
    pub fn add_liquidity(&mut self, amount_a: u128, amount_b: u128) -> Result<u128> {
        if amount_a == 0 || amount_b == 0 {
            bail!("Amounts must be > 0");
        }

        // Compute LP tokens to mint
        let lp_minted = if self.lp_total_supply == 0 {
            let k = checked_mul(amount_a, amount_b, "add_liquidity init k")?;
            integer_sqrt(k)
        } else {
            // min(amount_a / reserve_a, amount_b / reserve_b) * lp_supply
            let lp_a =
                checked_mul(amount_a, self.lp_total_supply, "add_liquidity lp_a")? / self.reserve_a;
            let lp_b =
                checked_mul(amount_b, self.lp_total_supply, "add_liquidity lp_b")? / self.reserve_b;
            lp_a.min(lp_b)
        };
        if lp_minted == 0 {
            bail!("LP mint result is zero");
        }

        self.reserve_a = checked_add(self.reserve_a, amount_a, "reserve_a + amount_a")?;
        self.reserve_b = checked_add(self.reserve_b, amount_b, "reserve_b + amount_b")?;
        self.lp_total_supply = checked_add(
            self.lp_total_supply,
            lp_minted,
            "lp_total_supply + lp_minted",
        )?;

        Ok(lp_minted)
    }

    /// Remove liquidity
    pub fn remove_liquidity(&mut self, lp_amount: u128) -> Result<(u128, u128)> {
        if lp_amount == 0 {
            bail!("LP amount must be > 0");
        }
        if lp_amount > self.lp_total_supply {
            bail!("LP amount exceeds supply");
        }

        // amount_a = lp_amount * reserve_a / lp_supply
        let amount_a = checked_mul(
            lp_amount,
            self.reserve_a,
            "remove_liquidity amount_a numerator",
        )? / self.lp_total_supply;
        let amount_b = checked_mul(
            lp_amount,
            self.reserve_b,
            "remove_liquidity amount_b numerator",
        )? / self.lp_total_supply;

        self.reserve_a = checked_sub(self.reserve_a, amount_a, "reserve_a - amount_a")?;
        self.reserve_b = checked_sub(self.reserve_b, amount_b, "reserve_b - amount_b")?;
        self.lp_total_supply = checked_sub(
            self.lp_total_supply,
            lp_amount,
            "lp_total_supply - lp_amount",
        )?;

        Ok((amount_a, amount_b))
    }

    /// Perform swap
    pub fn swap(&mut self, amount_in: u128, is_a_to_b: bool) -> Result<u128> {
        let amount_out = if is_a_to_b {
            let out = self.get_amount_out(amount_in, self.reserve_a, self.reserve_b)?;
            self.reserve_a = checked_add(self.reserve_a, amount_in, "swap a2b reserve_a + in")?;
            self.reserve_b = checked_sub(self.reserve_b, out, "swap a2b reserve_b - out")?;
            out
        } else {
            let out = self.get_amount_out(amount_in, self.reserve_b, self.reserve_a)?;
            self.reserve_b = checked_add(self.reserve_b, amount_in, "swap b2a reserve_b + in")?;
            self.reserve_a = checked_sub(self.reserve_a, out, "swap b2a reserve_a - out")?;
            out
        };

        Ok(amount_out)
    }
}

/// AMM manager
pub struct AMMManager {
    /// All pools
    pools: HashMap<[u8; 32], LiquidityPool>,
    /// User LP balances
    lp_balances: HashMap<(Address, [u8; 32]), u128>,
    /// Whether add_liquidity may auto-create missing pools.
    auto_create_pool: bool,
    /// Hard cap for total pool count.
    max_pools: usize,
}

impl Default for AMMManager {
    fn default() -> Self {
        Self {
            pools: HashMap::new(),
            lp_balances: HashMap::new(),
            auto_create_pool: false,
            max_pools: 1024,
        }
    }
}

impl AMMManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_auto_create_pool(&mut self, allow: bool) {
        self.auto_create_pool = allow;
    }

    pub fn set_max_pools(&mut self, max_pools: usize) -> Result<()> {
        if max_pools == 0 {
            bail!("max_pools must be > 0");
        }
        if max_pools < self.pools.len() {
            bail!(
                "max_pools={} below current pool count={}",
                max_pools,
                self.pools.len()
            );
        }
        self.max_pools = max_pools;
        Ok(())
    }

    /// Set global fee bps for all pools
    pub fn set_global_fee_bps(&mut self, fee_bps: u16) {
        let bps = fee_bps.min(10_000);
        for pool in self.pools.values_mut() {
            pool.fee_bps = bps;
        }
    }

    /// Set fee bps for a specific pool
    pub fn set_pool_fee_bps(&mut self, pool_id: &[u8; 32], fee_bps: u16) -> Result<()> {
        if fee_bps > 10_000 {
            bail!("Fee exceeds 100%");
        }
        let pool = self
            .pools
            .get_mut(pool_id)
            .ok_or_else(|| anyhow::anyhow!("Pool not found"))?;
        pool.fee_bps = fee_bps;
        Ok(())
    }

    /// Create a pool
    #[allow(clippy::too_many_arguments)]
    pub fn create_pool(
        &mut self,
        pool_id: [u8; 32],
        token_a: String,
        token_b: String,
        initial_a: u128,
        initial_b: u128,
        fee_bps: u16,
        provider: Address,
    ) -> Result<u128> {
        if self.pools.contains_key(&pool_id) {
            bail!("Pool already exists");
        }
        if self.pools.len() >= self.max_pools {
            bail!("Pool limit reached: max_pools={}", self.max_pools);
        }

        let pool = LiquidityPool::new(pool_id, token_a, token_b, initial_a, initial_b, fee_bps)?;
        let lp_supply = pool.lp_total_supply;

        self.pools.insert(pool_id, pool);
        self.lp_balances.insert((provider, pool_id), lp_supply);

        Ok(lp_supply)
    }

    /// Get pool by id
    pub fn get_pool(&self, pool_id: &[u8; 32]) -> Option<&LiquidityPool> {
        self.pools.get(pool_id)
    }

    /// Get LP balance
    pub fn lp_balance(&self, user: &Address, pool_id: &[u8; 32]) -> u128 {
        self.lp_balances
            .get(&(*user, *pool_id))
            .copied()
            .unwrap_or(0)
    }

    /// Add liquidity (auto-create pool if missing)
    pub fn add_liquidity(
        &mut self,
        pool_id: &[u8; 32],
        provider: Address,
        amount_a: u128,
        amount_b: u128,
    ) -> Result<u128> {
        // If pool missing, create with default 0.3% fee
        if !self.pools.contains_key(pool_id) {
            if !self.auto_create_pool {
                bail!("Pool not found and auto-create is disabled");
            }
            self.create_pool(
                *pool_id,
                "TOKEN_A".to_string(),
                "TOKEN_B".to_string(),
                amount_a,
                amount_b,
                30, // 0.3% default fee
                provider,
            )?;
            // create_pool already minted LP tokens, return immediately
            return Ok(self.lp_balance(&provider, pool_id));
        }

        let pool = self
            .pools
            .get_mut(pool_id)
            .ok_or_else(|| anyhow::anyhow!("Pool not found"))?;
        let lp_minted = pool.add_liquidity(amount_a, amount_b)?;

        let entry = self.lp_balances.entry((provider, *pool_id)).or_insert(0);
        *entry = checked_add(*entry, lp_minted, "lp balance + minted")?;

        Ok(lp_minted)
    }

    /// Remove liquidity
    pub fn remove_liquidity(
        &mut self,
        pool_id: &[u8; 32],
        provider: Address,
        lp_amount: u128,
    ) -> Result<(u128, u128)> {
        let lp_balance = self.lp_balance(&provider, pool_id);
        if lp_balance < lp_amount {
            bail!("Insufficient LP balance");
        }

        let pool = self
            .pools
            .get_mut(pool_id)
            .ok_or_else(|| anyhow::anyhow!("Pool not found"))?;
        let (amount_a, amount_b) = pool.remove_liquidity(lp_amount)?;

        let entry = self.lp_balances.entry((provider, *pool_id)).or_insert(0);
        *entry = checked_sub(*entry, lp_amount, "lp balance - burned")?;

        Ok((amount_a, amount_b))
    }

    /// Perform swap
    pub fn swap(
        &mut self,
        pool_id: &[u8; 32],
        amount_in: u128,
        is_a_to_b: bool,
        min_amount_out: u128,
    ) -> Result<u128> {
        let pool = self
            .pools
            .get_mut(pool_id)
            .ok_or_else(|| anyhow::anyhow!("Pool not found"))?;
        let amount_out = pool.swap(amount_in, is_a_to_b)?;

        if amount_out < min_amount_out {
            bail!(
                "Slippage exceeded: expected {}, got {}",
                min_amount_out,
                amount_out
            );
        }

        Ok(amount_out)
    }

    /// Quote output amount
    pub fn quote_amount_out(
        &self,
        pool_id: &[u8; 32],
        amount_in: u128,
        is_a_to_b: bool,
    ) -> Result<u128> {
        let pool = self
            .pools
            .get(pool_id)
            .ok_or_else(|| anyhow::anyhow!("Pool not found"))?;
        if is_a_to_b {
            pool.get_amount_out(amount_in, pool.reserve_a, pool.reserve_b)
        } else {
            pool.get_amount_out(amount_in, pool.reserve_b, pool.reserve_a)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(id: u8) -> Address {
        Address::from_bytes([id; 32])
    }

    fn pool_id(id: u8) -> [u8; 32] {
        [id; 32]
    }

    #[test]
    fn test_pool_creation() {
        let pool = LiquidityPool::new(
            pool_id(1),
            "SVM".into(),
            "USDT".into(),
            1_000_000,
            10_000,
            30,
        )
        .expect("create pool");

        assert_eq!(pool.reserve_a, 1_000_000);
        assert_eq!(pool.reserve_b, 10_000);
        // LP = sqrt(1_000_000 * 10_000) = 100_000
        assert_eq!(pool.lp_total_supply, 100_000);
    }

    #[test]
    fn test_swap_calculation() {
        let mut pool = LiquidityPool::new(
            pool_id(1),
            "SVM".into(),
            "USDT".into(),
            1_000_000,
            10_000,
            30, // 0.3% fee
        )
        .expect("create pool");

        // Swap 1000 SVM to USDT
        let amount_out = pool.swap(1_000, true).expect("swap");

        // Theory: 1000 * 0.997 * 10_000 / (1_000_000 + 1000 * 0.997) ≈ 9.96
        assert!(amount_out > 0 && amount_out < 10);
    }

    #[test]
    fn test_add_remove_liquidity() {
        let mut pool = LiquidityPool::new(
            pool_id(1),
            "SVM".into(),
            "USDT".into(),
            1_000_000,
            10_000,
            30,
        )
        .expect("create pool");

        let initial_lp = pool.lp_total_supply;

        // Add liquidity
        let lp_minted = pool.add_liquidity(100_000, 1_000).expect("add liquidity");
        assert_eq!(lp_minted, initial_lp / 10); // 10% increase

        // Remove liquidity
        let (amount_a, amount_b) = pool.remove_liquidity(lp_minted).expect("remove liquidity");
        assert_eq!(amount_a, 100_000);
        assert_eq!(amount_b, 1_000);
    }

    #[test]
    fn test_amm_manager() {
        let mut manager = AMMManager::new();
        let provider = addr(1);
        let pid = pool_id(1);

        // 创建池
        let lp = manager
            .create_pool(
                pid,
                "SVM".into(),
                "USDT".into(),
                1_000_000,
                10_000,
                30,
                provider,
            )
            .expect("create pool");

        assert_eq!(manager.lp_balance(&provider, &pid), lp);

        // 添加流动性
        manager
            .add_liquidity(&pid, provider, 100_000, 1_000)
            .expect("add liquidity");

        // Perform swap
        let amount_out = manager.swap(&pid, 1_000, true, 0).expect("swap");
        assert!(amount_out > 0);
    }

    #[test]
    fn test_price_impact() {
        let mut pool = LiquidityPool::new(
            pool_id(1),
            "SVM".into(),
            "USDT".into(),
            1_000_000,
            10_000,
            30,
        )
        .expect("create pool");

        let initial_price_scaled = pool.price_scaled(1_000_000).expect("initial price");

        // Large swap causes price impact
        pool.swap(100_000, true).expect("swap");

        let new_price_scaled = pool.price_scaled(1_000_000).expect("new price");
        assert!(new_price_scaled < initial_price_scaled); // SVM devalues relative to USDT
    }

    #[test]
    fn test_slippage_protection() {
        let mut manager = AMMManager::new();
        let pid = pool_id(1);

        manager
            .create_pool(
                pid,
                "SVM".into(),
                "USDT".into(),
                1_000_000,
                10_000,
                30,
                addr(1),
            )
            .expect("create pool");

        // Expect at least 20, but actual is 9.x
        let result = manager.swap(&pid, 1_000, true, 20);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Slippage exceeded"));
    }

    #[test]
    fn test_pool_auto_create_disabled_by_default() {
        let mut manager = AMMManager::new();
        let pid = pool_id(9);
        let err = manager
            .add_liquidity(&pid, addr(1), 1_000, 1_000)
            .expect_err("auto-create must be disabled by default");
        assert!(err.to_string().contains("auto-create is disabled"));
    }

    #[test]
    fn test_pool_limit_enforced() {
        let mut manager = AMMManager::new();
        manager.set_max_pools(1).expect("set max pools");
        manager.set_auto_create_pool(true);
        manager
            .add_liquidity(&pool_id(1), addr(1), 1_000, 1_000)
            .expect("create first pool");
        let err = manager
            .add_liquidity(&pool_id(2), addr(2), 1_000, 1_000)
            .expect_err("second pool should exceed limit");
        assert!(err.to_string().contains("Pool limit reached"));
    }
}
