//! 经济模块集成示例
//!
//! 展示 SuperVM 完整经济系统的协同工作:
//! - AMM: 市场定价
//! - NAV Redemption: 刚性兑付
//! - Bonds: 债券融资
//! - CDP: 稳定币铸造

use anyhow::Result;

use crate::{
    amm::AMMManager,
    bonds::{BondManager, BondType},
    cdp::{CdpManager, CollateralType},
    nav_redemption::NavRedemptionManager,
    types::Address,
};

/// 简化的经济系统
pub struct EconomicSystem {
    /// AMM 管理器
    pub amm: AMMManager,
    /// NAV 赎回管理器
    pub nav_redemption: NavRedemptionManager,
    /// 债券管理器
    pub bonds: BondManager,
    /// CDP 管理器
    pub cdp: CdpManager,
    /// 当前区块日期
    pub current_day: u64,
}

impl EconomicSystem {
    /// 创建新的经济系统
    pub fn new() -> Self {
        let mut amm = AMMManager::new();
        // Integration demo keeps historical auto-create behavior;
        // production AMMManager default remains hardened (auto_create_pool=false).
        amm.set_auto_create_pool(true);

        let nav_redemption = NavRedemptionManager::new(
            1_000_000, // 每日 1M 配额
            5_000,     // 50% 最低储备率
            7,         // T+7 结算
        );

        let bonds = BondManager::new();

        let cdp = CdpManager::new(500); // 5% 清算罚金

        Self {
            amm,
            nav_redemption,
            bonds,
            cdp,
            current_day: 1,
        }
    }

    /// 场景 1: 初始化 AMM 流动性池
    pub fn scenario_1_init_liquidity(&mut self, lp_provider: Address) -> Result<u128> {
        let pool_id = [1u8; 32]; // TOKEN/USDT pool ID

        // LP 提供 10,000 Token 和 100,000 USDT
        // add_liquidity(pool_id, provider, amount_a, amount_b)
        let lp_tokens = self.amm.add_liquidity(
            &pool_id,
            lp_provider,
            10_000 * 1_000_000,  // 10k Token (假设 1e6 精度)
            100_000 * 1_000_000, // 100k USDT
        )?;

        println!("✅ 场景1: AMM 流动性池初始化");
        println!("   LP Token 数量: {}", lp_tokens / 1_000_000);
        println!("   初始价格: 1 Token = 10 USDT");

        Ok(lp_tokens)
    }

    /// 场景 2: 国库发行债券融资
    pub fn scenario_2_issue_bonds(&mut self, investor: Address) -> Result<()> {
        let bond_id = [1u8; 32];

        self.bonds.issue_bond(
            bond_id,
            BondType::OneYear,
            investor,
            10_000 * 1_000_000, // 面值
            self.current_day,
            10_500, // 105% 溢价发行
        )?;

        println!("✅ 场景2: 国库发行债券");
        println!("   类型: 1年期 (8% 年化)");
        println!("   面值: 10,000 Token");
        println!("   发行价: 105% NAV");
        println!("   到期价值: 10,800 Token");

        Ok(())
    }

    /// 场景 3: 用户抵押铸造稳定币
    pub fn scenario_3_mint_stablecoin(&mut self, user: Address) -> Result<()> {
        let cdp_id = [1u8; 32];

        self.cdp.open_cdp(
            cdp_id,
            user,
            CollateralType::MainnetToken,
            1_000 * 1_000_000,  // 1000 Token
            10_000 * 1_000_000, // 价值 10k USDT
            6_666 * 1_000_000,  // 铸造 6666 稳定币
            self.current_day,
        )?;

        println!("✅ 场景3: CDP 抵押铸币");
        println!("   抵押品: 1,000 Token");
        println!("   铸造稳定币: 6,666 USDT");
        println!("   抵押率: 150%");

        Ok(())
    }

    /// 场景 4: NAV 刚性兑付
    pub fn scenario_4_nav_redemption(&mut self, user: Address) -> Result<()> {
        let m1_supply = 100_000 * 1_000_000;
        let reserve_value = 1_000_000 * 1_000_000;
        let treasury_balance = 500_000 * 1_000_000;

        self.nav_redemption.record_nav_snapshot(
            self.current_day,
            reserve_value,
            treasury_balance,
            m1_supply,
        )?;

        let req_id = [1u8; 32];
        let request = self.nav_redemption.submit_redemption(
            req_id,
            user,
            1_000 * 1_000_000,
            self.current_day,
        )?;

        println!("✅ 场景4: NAV 刚性兑付");
        println!("   NAV: 15 USDT/Token");
        println!("   赎回数量: 1,000 Token");
        println!("   预期金额: 15,000 USDT");
        println!("   结算日期: T+7 (day {})", request.execution_day);

        Ok(())
    }

    /// 场景 5: 价格波动触发 CDP 清算
    pub fn scenario_5_cdp_liquidation(&mut self) -> Result<()> {
        let cdp_id = [2u8; 32];
        let user = Address::from_bytes([10u8; 32]);

        self.cdp.open_cdp(
            cdp_id,
            user,
            CollateralType::MainnetToken,
            1_000 * 1_000_000,
            10_000 * 1_000_000,
            6_666 * 1_000_000,
            self.current_day,
        )?;

        println!("✅ 场景5: CDP 清算流程");
        println!("   初始抵押率: 150%");

        self.cdp
            .update_collateral_price(cdp_id, 8_000 * 1_000_000)?;

        println!("   价格下跌: 10 → 8 USDT/Token");
        println!("   新抵押率: 120% < 125% 清算线");

        let (seized, penalty) = self.cdp.liquidate_cdp(cdp_id)?;

        println!("   清算执行:");
        println!("     - 没收抵押品: {} Token", seized / 1_000_000);
        println!("     - 清算罚金: {} USDT", penalty / 1_000_000);

        Ok(())
    }
}

impl Default for EconomicSystem {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integrated_scenarios() {
        let mut system = EconomicSystem::new();

        // 场景 1: 初始化流动性
        let lp = Address::from_bytes([100u8; 32]);
        system.scenario_1_init_liquidity(lp).expect("s1");

        // 场景 2: 债券发行
        let investor = Address::from_bytes([103u8; 32]);
        system.scenario_2_issue_bonds(investor).expect("s2");

        // 场景 3: CDP 铸币
        let user2 = Address::from_bytes([104u8; 32]);
        system.scenario_3_mint_stablecoin(user2).expect("s3");

        // 场景 4: NAV 赎回
        let user3 = Address::from_bytes([105u8; 32]);
        system.scenario_4_nav_redemption(user3).expect("s4");

        // 场景 5: CDP 清算
        system.scenario_5_cdp_liquidation().expect("s5");

        println!("\n🎉 所有场景测试通过!");
    }

    #[test]
    fn test_economic_stats() {
        let mut system = EconomicSystem::new();

        let lp = Address::from_bytes([100u8; 32]);
        system.scenario_1_init_liquidity(lp).expect("s1");

        let investor = Address::from_bytes([103u8; 32]);
        system.scenario_2_issue_bonds(investor).expect("s2");

        let user = Address::from_bytes([104u8; 32]);
        system.scenario_3_mint_stablecoin(user).expect("s3");

        // 检查统计数据
        assert_eq!(system.bonds.total_issued(), 10_000 * 1_000_000);
        assert_eq!(system.cdp.total_supply(), 6_666 * 1_000_000);

        let (bond_count, bond_principal) = system.bonds.get_stats(BondType::OneYear);
        assert_eq!(bond_count, 1);
        assert_eq!(bond_principal, 10_000 * 1_000_000);

        let (cdp_count, cdp_coll, cdp_debt) = system.cdp.get_stats(CollateralType::MainnetToken);
        assert_eq!(cdp_count, 1);
        assert_eq!(cdp_coll, 10_000 * 1_000_000);
        assert_eq!(cdp_debt, 6_666 * 1_000_000);
    }
}
