//! ForeignPayment processor implementation
//!
//! Responsibilities:
//! 1. Receive foreign payments (BTC/ETH/USDT etc.) → 100% collected to reserve pool
//! 2. Compute equivalent Token amount using AMM rates
//! 3. Pay Token to miners from treasury/M0 pool
//! 4. Miners may hold or swap back to foreign currency via AMM
//! 5. Track reserve utilization and miner hold preference

use std::collections::HashMap;

use anyhow::Result;

use crate::{
    foreign_payment::{ForeignPayment, ForeignPaymentProcessor, ForeignPaymentStats, MinerPayment},
    types::Address,
};

/// AMM exchange rate adapter (placeholder, will connect to real AMM)
pub trait ExchangeRateProvider {
    /// Query foreign → Token rate
    /// Returns: (Tokens per foreign unit, slippage bps)
    fn get_exchange_rate(&self, currency: &str) -> Result<(f64, u16)>;
}

/// Simple fixed-rate provider (test/placeholder)
pub struct MockExchangeRateProvider {
    rates: HashMap<String, f64>,
}

impl MockExchangeRateProvider {
    pub fn new() -> Self {
        let mut rates = HashMap::new();
        // Placeholder rates: 1 BTC = 100,000 Token, 1 ETH = 5,000 Token, 1 USDT = 10 Token
        rates.insert("BTC".to_string(), 100_000.0);
        rates.insert("ETH".to_string(), 5_000.0);
        rates.insert("USDT".to_string(), 10.0);
        Self { rates }
    }

    pub fn set_rate(&mut self, currency: &str, rate: f64) {
        self.rates.insert(currency.to_string(), rate);
    }
}

impl ExchangeRateProvider for MockExchangeRateProvider {
    fn get_exchange_rate(&self, currency: &str) -> Result<(f64, u16)> {
        let rate = self
            .rates
            .get(currency)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Unsupported currency: {}", currency))?;
        Ok((rate, 50)) // 0.5% slippage range
    }
}

impl Default for MockExchangeRateProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// ForeignPayment processor config
pub struct ForeignPaymentConfig<R: ExchangeRateProvider> {
    /// Rate provider
    pub rate_provider: R,
    /// Treasury address (Token source)
    pub treasury_address: Address,
    /// M0 pool address (backup source)
    pub m0_pool_address: Address,
}

/// ForeignPayment processor implementation
pub struct ForeignPaymentProcessorImpl<R: ExchangeRateProvider> {
    rate_provider: R,
    #[allow(dead_code)]
    treasury_address: Address,
    #[allow(dead_code)]
    m0_pool_address: Address,
    stats: ForeignPaymentStats,
}

impl<R: ExchangeRateProvider> ForeignPaymentProcessorImpl<R> {
    pub fn new(config: ForeignPaymentConfig<R>) -> Self {
        Self {
            rate_provider: config.rate_provider,
            treasury_address: config.treasury_address,
            m0_pool_address: config.m0_pool_address,
            stats: ForeignPaymentStats::default(),
        }
    }

    /// Get current timestamp (seconds)
    fn now(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// 查询统计数据
    pub fn stats(&self) -> &ForeignPaymentStats {
        &self.stats
    }
}

impl<R: ExchangeRateProvider> ForeignPaymentProcessor for ForeignPaymentProcessorImpl<R> {
    fn process_foreign_payment(
        &mut self,
        payment: ForeignPayment,
        miner: Address,
    ) -> Result<MinerPayment, String> {
        // 1. Validate payment
        if payment.amount == 0 {
            return Err("Payment amount must be > 0".into());
        }

        // 2. Collect foreign currency to reserve pool
        self.collect_to_reserve(payment.clone())
            .map_err(|e| e.to_string())?;

        // 3. Compute equivalent Token amount
        let (token_amount, exchange_rate) = self
            .calculate_token_equivalent(&payment.currency, payment.amount)
            .map_err(|e| e.to_string())?;

        // 4. Pay Token to miner (would call MainnetToken::transfer or mint)
        // Record payment only; actual transfer handled by upper layer
        let payment_record = MinerPayment {
            miner,
            token_amount,
            equivalent_foreign: payment.amount,
            foreign_currency: payment.currency.clone(),
            exchange_rate,
            timestamp: self.now(),
        };

        // 5. Update stats
        self.stats.total_token_paid = self.stats.total_token_paid.saturating_add(token_amount);

        Ok(payment_record)
    }

    fn calculate_token_equivalent(
        &self,
        currency: &str,
        foreign_amount: u128,
    ) -> Result<(u128, f64), String> {
        let (rate, _slippage) = self
            .rate_provider
            .get_exchange_rate(currency)
            .map_err(|e| e.to_string())?;

        // token_amount = foreign_amount * rate
        // Note: precision conversion needed in production
        let token_amount = (foreign_amount as f64 * rate) as u128;

        Ok((token_amount, rate))
    }

    fn collect_to_reserve(&mut self, payment: ForeignPayment) -> Result<(), String> {
        if payment.amount == 0 {
            return Err("Amount must be > 0".into());
        }

        // Update reserve stats
        let entry = self
            .stats
            .total_collected
            .entry(payment.currency.clone())
            .or_insert(0);
        *entry = entry.saturating_add(payment.amount);

        let reserve_entry = self
            .stats
            .current_reserves
            .entry(payment.currency.clone())
            .or_insert(0);
        *reserve_entry = reserve_entry.saturating_add(payment.amount);

        Ok(())
    }

    fn pay_miner_in_token(
        &mut self,
        miner: Address,
        amount: u128,
        payment_info: ForeignPayment,
    ) -> Result<MinerPayment, String> {
        // Simplified: return payment record directly
        // Production steps:
        // 1. Check treasury/M0 pool balances
        // 2. Call MainnetToken::transfer or mint
        // 3. Record payment event

        let (exchange_rate, _slippage) = self
            .rate_provider
            .get_exchange_rate(&payment_info.currency)
            .map_err(|e| e.to_string())?;

        Ok(MinerPayment {
            miner,
            token_amount: amount,
            equivalent_foreign: payment_info.amount,
            foreign_currency: payment_info.currency,
            exchange_rate,
            timestamp: self.now(),
        })
    }

    fn miner_swap_to_foreign(
        &mut self,
        _miner: Address,
        token_amount: u128,
        target_currency: &str,
        min_receive: u128,
    ) -> Result<u128, String> {
        // 1. Query rate
        let (swappable_amount, _rate) = self.get_swappable_amount(token_amount, target_currency)?;

        // 2. Slippage protection
        if swappable_amount < min_receive {
            return Err(format!(
                "Slippage too high: expected {}, got {}",
                min_receive, swappable_amount
            ));
        }

        // 3. Check reserve sufficiency
        let current_reserve = self
            .stats
            .current_reserves
            .get(target_currency)
            .copied()
            .unwrap_or(0);
        if current_reserve < swappable_amount {
            return Err(format!(
                "Insufficient reserve: has {}, need {}",
                current_reserve, swappable_amount
            ));
        }

        // 4. Execute swap
        // - Burn miner's token (MainnetToken::burn)
        // - Transfer foreign currency from reserve to miner
        // Update stats only here

        let reserve_entry = self
            .stats
            .current_reserves
            .entry(target_currency.to_string())
            .or_insert(0);
        *reserve_entry = reserve_entry.saturating_sub(swappable_amount);

        let swapped_entry = self
            .stats
            .total_swapped_out
            .entry(target_currency.to_string())
            .or_insert(0);
        *swapped_entry = swapped_entry.saturating_add(swappable_amount);

        Ok(swappable_amount)
    }

    fn get_swappable_amount(
        &self,
        token_amount: u128,
        target_currency: &str,
    ) -> Result<(u128, f64), String> {
        let (rate, _slippage) = self
            .rate_provider
            .get_exchange_rate(target_currency)
            .map_err(|e| e.to_string())?;

        // foreign_amount = token_amount / rate
        let foreign_amount = (token_amount as f64 / rate) as u128;

        Ok((foreign_amount, rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foreign_payment::ServiceType;

    fn addr(id: u8) -> Address {
        Address::from_bytes([id; 32])
    }

    fn build_processor() -> ForeignPaymentProcessorImpl<MockExchangeRateProvider> {
        let rate_provider = MockExchangeRateProvider::new();
        ForeignPaymentProcessorImpl::new(ForeignPaymentConfig {
            rate_provider,
            treasury_address: addr(200),
            m0_pool_address: addr(201),
        })
    }

    #[test]
    fn test_foreign_payment_processing() {
        let mut processor = build_processor();
        let miner = addr(50);

        let payment = ForeignPayment {
            currency: "BTC".to_string(),
            amount: 100_000_000, // 1 BTC (satoshi)
            payer: "bc1q...".to_string(),
            service_type: ServiceType::Gas,
        };

        let result = processor
            .process_foreign_payment(payment, miner)
            .expect("process");

        // 1 BTC = 100,000 Token (按占位汇率)
        assert_eq!(result.token_amount, 10_000_000_000_000);
        assert_eq!(result.foreign_currency, "BTC");
        assert_eq!(result.exchange_rate, 100_000.0);
    }

    #[test]
    fn test_collect_to_reserve() {
        let mut processor = build_processor();

        let payment = ForeignPayment {
            currency: "ETH".to_string(),
            amount: 10_000_000_000_000_000_000, // 10 ETH (wei)
            payer: "0x...".to_string(),
            service_type: ServiceType::TransactionFee,
        };

        processor.collect_to_reserve(payment).expect("collect");

        assert_eq!(
            processor
                .stats()
                .current_reserves
                .get("ETH")
                .copied()
                .unwrap_or(0),
            10_000_000_000_000_000_000
        );
        assert_eq!(
            processor
                .stats()
                .total_collected
                .get("ETH")
                .copied()
                .unwrap_or(0),
            10_000_000_000_000_000_000
        );
    }

    #[test]
    fn test_token_equivalent_calculation() {
        let processor = build_processor();

        // 1 USDT = 10 Token
        let (token_amount, rate) = processor
            .calculate_token_equivalent("USDT", 1_000_000) // 1 USDT (6 decimals)
            .expect("calculate");

        assert_eq!(token_amount, 10_000_000);
        assert_eq!(rate, 10.0);
    }

    #[test]
    fn test_miner_swap_to_foreign() {
        let mut processor = build_processor();

        // 先模拟收集一些 BTC 到储备
        let payment = ForeignPayment {
            currency: "BTC".to_string(),
            amount: 200_000_000, // 2 BTC
            payer: "bc1q...".to_string(),
            service_type: ServiceType::Gas,
        };
        processor.collect_to_reserve(payment).expect("collect");

        let miner = addr(60);
        // 矿工用 10,000,000 Token 兑换 BTC
        // 10,000,000 / 100,000 = 100 (按 1 BTC = 100,000 Token 汇率)
        let swapped = processor
            .miner_swap_to_foreign(miner, 10_000_000, "BTC", 90)
            .expect("swap");

        assert_eq!(swapped, 100);
        assert_eq!(
            processor
                .stats()
                .current_reserves
                .get("BTC")
                .copied()
                .unwrap_or(0),
            200_000_000 - 100
        );
        assert_eq!(
            processor
                .stats()
                .total_swapped_out
                .get("BTC")
                .copied()
                .unwrap_or(0),
            100
        );
    }

    #[test]
    fn test_swap_slippage_protection() {
        let mut processor = build_processor();

        let payment = ForeignPayment {
            currency: "ETH".to_string(),
            amount: 5_000_000_000_000_000_000, // 5 ETH
            payer: "0x...".to_string(),
            service_type: ServiceType::Gas,
        };
        processor.collect_to_reserve(payment).expect("collect");

        let miner = addr(70);
        // 期望兑换出 2 ETH, 但实际只能换 1 ETH
        let result =
            processor.miner_swap_to_foreign(miner, 5_000_000, "ETH", 2_000_000_000_000_000_000);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Slippage too high"));
    }

    #[test]
    fn test_insufficient_reserve() {
        let mut processor = build_processor();

        // 仅有 1 BTC 储备
        let payment = ForeignPayment {
            currency: "BTC".to_string(),
            amount: 100_000_000,
            payer: "bc1q...".to_string(),
            service_type: ServiceType::Gas,
        };
        processor.collect_to_reserve(payment).expect("collect");

        let miner = addr(80);
        // 尝试兑换 2 BTC (需要 200 BTC 的 Token)
        // 2 BTC = 200,000,000 satoshi, 需要 200,000,000 * 100,000 = 20,000,000,000,000 Token
        let result = processor.miner_swap_to_foreign(miner, 20_000_000_000_000, "BTC", 190_000_000);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient reserve"));
    }

    #[test]
    fn test_reserve_ratio_calculation() {
        let mut processor = build_processor();

        // 收集 10 ETH
        let payment1 = ForeignPayment {
            currency: "ETH".to_string(),
            amount: 10_000_000_000_000_000_000,
            payer: "0x...".to_string(),
            service_type: ServiceType::Gas,
        };
        processor.collect_to_reserve(payment1).expect("collect");

        // 矿工兑换 2 ETH
        // 2 ETH = 2,000,000,000,000,000,000 wei, 需要 2 * 5,000 = 10,000 Token (按 1 ETH = 5,000 Token)
        // 实际单位换算: 2,000,000,000,000,000,000 wei 需要 10,000,000,000,000,000,000,000 Token
        processor
            .miner_swap_to_foreign(addr(90), 10_000_000_000_000_000_000_000, "ETH", 0)
            .expect("swap");

        // 储备率 = 8 / 10 = 80%
        let ratio = processor.stats().calculate_reserve_ratio("ETH");
        assert!((ratio - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_hold_preference_calculation() {
        let mut processor = build_processor();

        // 收集 100 USDT
        let payment = ForeignPayment {
            currency: "USDT".to_string(),
            amount: 100_000_000, // 100 USDT (6 decimals)
            payer: "0x...".to_string(),
            service_type: ServiceType::Gas,
        };
        processor.collect_to_reserve(payment).expect("collect");

        // 矿工兑换 20 USDT
        processor
            .miner_swap_to_foreign(addr(95), 200_000_000, "USDT", 0)
            .expect("swap");

        // 持有倾向 = 1 - (20 / 100) = 80%
        let preference = processor.stats().calculate_hold_preference("USDT");
        assert!((preference - 0.8).abs() < 0.01);
    }
}
