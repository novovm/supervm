/// 外币支付处理模块
///
/// 职责:
/// 1. 接收外部链用户的Gas/手续费支付(BTC/ETH/USDT等)
/// 2. 将外币100%归集到外汇储备池
/// 3. 用MainnetToken等值支付给矿主
/// 4. 矿主可选择持有或通过AMM兑换外币
use crate::types::Address;

/// Fixed-point scale used for foreign exchange rates and ratio metrics.
/// 1.0 == 1_000_000
pub const FOREIGN_RATE_SCALE: u128 = 1_000_000;

/// 外币支付信息
#[derive(Debug, Clone)]
pub struct ForeignPayment {
    /// 支付币种
    pub currency: String, // "BTC", "ETH", "USDT"等
    /// 支付金额(单位: 对应币种最小单位, 如satoshi, wei)
    pub amount: u128,
    /// 付款人地址(外部链地址)
    pub payer: String,
    /// 服务类型
    pub service_type: ServiceType,
}

/// 服务类型
#[derive(Debug, Clone)]
pub enum ServiceType {
    /// Gas费
    Gas,
    /// 交易手续费
    TransactionFee,
    /// 智能合约执行费
    ContractExecution,
    /// 跨链桥接费
    CrossChainBridge,
}

/// 矿主支付凭证
#[derive(Debug, Clone)]
pub struct MinerPayment {
    /// 矿主地址
    pub miner: Address,
    /// 获得的MainnetToken数量
    pub token_amount: u128,
    /// 等值外币金额
    pub equivalent_foreign: u128,
    /// 外币币种
    pub foreign_currency: String,
    /// 支付时的汇率(Token/ForeignCurrency, fixed-point scaled by FOREIGN_RATE_SCALE)
    pub exchange_rate_scaled: u128,
    /// 支付时间戳
    pub timestamp: u64,
}

/// 外币支付处理器接口
pub trait ForeignPaymentProcessor {
    /// 处理外币支付
    ///
    /// 流程:
    /// 1. 验证外币支付有效性
    /// 2. 归集外币到储备池
    /// 3. 查询AMM汇率
    /// 4. 从国库支付等值MainnetToken给矿主
    ///
    /// 参数:
    /// - payment: 外币支付信息
    /// - miner: 矿主地址
    ///
    /// 返回: 矿主支付凭证
    fn process_foreign_payment(
        &mut self,
        payment: ForeignPayment,
        miner: Address,
    ) -> Result<MinerPayment, String>;

    /// 查询外币应支付的Token数量
    ///
    /// 根据AMM汇率计算:
    /// token_amount = foreign_amount * exchange_rate
    ///
    /// 参数:
    /// - currency: 外币币种
    /// - foreign_amount: 外币金额
    ///
    /// 返回: (Token数量, 当前汇率(定点))
    fn calculate_token_equivalent(
        &self,
        currency: &str,
        foreign_amount: u128,
    ) -> Result<(u128, u128), String>;

    /// 归集外币到储备池
    ///
    /// 外币100%进入储备,不分配给矿主
    /// 增强AMM池子流动性
    fn collect_to_reserve(&mut self, payment: ForeignPayment) -> Result<(), String>;

    /// 从国库支付Token给矿主
    ///
    /// 来源优先级:
    /// 1. M0储备(预留的矿工激励池)
    /// 2. 市场回购的M1(用20%手续费储备买回)
    fn pay_miner_in_token(
        &mut self,
        miner: Address,
        amount: u128,
        payment_info: ForeignPayment,
    ) -> Result<MinerPayment, String>;

    /// 矿主兑换Token为外币
    ///
    /// 通过AMM池子按市场汇率兑换
    /// 流程:
    /// 1. 矿主发起兑换请求
    /// 2. 销毁矿主的Token
    /// 3. 从储备池转出外币给矿主
    /// 4. AMM池子自动调整汇率
    fn miner_swap_to_foreign(
        &mut self,
        miner: Address,
        token_amount: u128,
        target_currency: &str,
        min_receive: u128, // 最小接收外币数量(滑点保护)
    ) -> Result<u128, String>; // 返回实际收到的外币数量

    /// 查询矿主可兑换的外币数量
    ///
    /// 根据当前AMM汇率和Token余额计算
    fn get_swappable_amount(
        &self,
        token_amount: u128,
        target_currency: &str,
    ) -> Result<(u128, u128), String>; // (可兑换外币数量, 当前汇率(定点))
}

/// 外币支付统计
#[derive(Debug, Clone, Default)]
pub struct ForeignPaymentStats {
    /// 各币种累计收入
    pub total_collected: std::collections::HashMap<String, u128>,
    /// 已支付给矿主的Token总量
    pub total_token_paid: u128,
    /// 矿主兑换走的外币总量(按币种)
    pub total_swapped_out: std::collections::HashMap<String, u128>,
    /// 当前储备余额(按币种)
    pub current_reserves: std::collections::HashMap<String, u128>,
}

impl ForeignPaymentStats {
    /// 计算储备利用率
    ///
    /// reserve_ratio_scaled = current_reserves / total_collected (scaled by FOREIGN_RATE_SCALE)
    /// 越高说明矿主更倾向持有Token(看好生态)
    pub fn calculate_reserve_ratio_scaled(&self, currency: &str) -> u128 {
        let total = self.total_collected.get(currency).unwrap_or(&0);
        let current = self.current_reserves.get(currency).unwrap_or(&0);

        if *total == 0 {
            return 0;
        }

        scale_ratio(*current, *total)
    }

    /// 计算Token持有倾向
    ///
    /// hold_preference_scaled = 1.0 - (total_swapped / total_collected), scaled by FOREIGN_RATE_SCALE
    /// 越高说明矿主更愿意持有Token而非兑换
    pub fn calculate_hold_preference_scaled(&self, currency: &str) -> u128 {
        let total = self.total_collected.get(currency).unwrap_or(&0);
        let swapped = self.total_swapped_out.get(currency).unwrap_or(&0);

        if *total == 0 {
            return 0;
        }

        let swapped_ratio = scale_ratio(*swapped, *total).min(FOREIGN_RATE_SCALE);
        FOREIGN_RATE_SCALE.saturating_sub(swapped_ratio)
    }
}

fn scale_ratio(numerator: u128, denominator: u128) -> u128 {
    if denominator == 0 {
        return 0;
    }
    let whole = numerator / denominator;
    let rem = numerator % denominator;
    let whole_scaled = whole.saturating_mul(FOREIGN_RATE_SCALE);
    let rem_scaled = rem.saturating_mul(FOREIGN_RATE_SCALE) / denominator;
    whole_scaled.saturating_add(rem_scaled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reserve_ratio() {
        let mut stats = ForeignPaymentStats::default();
        stats.total_collected.insert("BTC".to_string(), 1000000); // 0.01 BTC
        stats.current_reserves.insert("BTC".to_string(), 800000); // 0.008 BTC

        let ratio_scaled = stats.calculate_reserve_ratio_scaled("BTC");
        assert_eq!(ratio_scaled, 800_000); // 80%储备率
    }

    #[test]
    fn test_hold_preference() {
        let mut stats = ForeignPaymentStats::default();
        stats
            .total_collected
            .insert("ETH".to_string(), 10_000_000_000_000_000_000); // 10 ETH
        stats
            .total_swapped_out
            .insert("ETH".to_string(), 2_000_000_000_000_000_000); // 2 ETH

        let preference_scaled = stats.calculate_hold_preference_scaled("ETH");
        assert_eq!(preference_scaled, 800_000); // 80%矿主选择持有
    }
}
