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

const FOREIGN_RATE_SCALE: u128 = 1_000_000;

fn scaled_to_rate_f64(rate_scaled: u128) -> f64 {
    rate_scaled as f64 / FOREIGN_RATE_SCALE as f64
}

fn rate_to_scaled_f64(rate: f64) -> Result<u128> {
    if !rate.is_finite() || rate <= 0.0 {
        return Err(anyhow::anyhow!("invalid exchange rate: {}", rate));
    }
    let scaled = (rate * FOREIGN_RATE_SCALE as f64).round();
    if scaled <= 0.0 || scaled > u128::MAX as f64 {
        return Err(anyhow::anyhow!("invalid scaled exchange rate: {}", scaled));
    }
    Ok(scaled as u128)
}

fn parse_rate_str_to_scaled(raw: &str) -> Result<u128> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(anyhow::anyhow!("rate cannot be empty"));
    }
    if value.starts_with('-') {
        return Err(anyhow::anyhow!("rate must be > 0"));
    }
    let mut parts = value.split('.');
    let int_part = parts.next().unwrap_or("");
    let frac_part = parts.next();
    if parts.next().is_some() {
        return Err(anyhow::anyhow!("invalid decimal rate: {}", raw));
    }
    if int_part.is_empty() || !int_part.chars().all(|c| c.is_ascii_digit()) {
        return Err(anyhow::anyhow!("invalid integer part in rate: {}", raw));
    }
    let int_num = int_part
        .parse::<u128>()
        .map_err(|_| anyhow::anyhow!("rate integer part overflow: {}", raw))?;
    let int_scaled = int_num
        .checked_mul(FOREIGN_RATE_SCALE)
        .ok_or_else(|| anyhow::anyhow!("rate overflow: {}", raw))?;

    let frac_scaled = if let Some(frac) = frac_part {
        if frac.is_empty() || !frac.chars().all(|c| c.is_ascii_digit()) {
            return Err(anyhow::anyhow!("invalid fractional part in rate: {}", raw));
        }
        if frac.len() > 6 {
            return Err(anyhow::anyhow!(
                "rate precision exceeds 6 decimals (scale={}): {}",
                FOREIGN_RATE_SCALE,
                raw
            ));
        }
        let frac_num = frac
            .parse::<u128>()
            .map_err(|_| anyhow::anyhow!("rate fractional part overflow: {}", raw))?;
        let pad = 6usize.saturating_sub(frac.len());
        let mut scale = 1u128;
        for _ in 0..pad {
            scale = scale.saturating_mul(10);
        }
        frac_num
            .checked_mul(scale)
            .ok_or_else(|| anyhow::anyhow!("rate fractional scaling overflow: {}", raw))?
    } else {
        0
    };

    let total = int_scaled
        .checked_add(frac_scaled)
        .ok_or_else(|| anyhow::anyhow!("rate total overflow: {}", raw))?;
    if total == 0 {
        return Err(anyhow::anyhow!("rate must be > 0"));
    }
    Ok(total)
}

/// AMM exchange rate adapter (placeholder, will connect to real AMM)
pub trait ExchangeRateProvider {
    /// Query foreign → Token rate (scaled integer, deterministic).
    /// Returns: (tokens_per_foreign_unit_scaled, slippage_bps)
    fn get_exchange_rate_scaled(&self, currency: &str) -> Result<(u128, u16)>;

    /// Compatibility/display API.
    fn get_exchange_rate(&self, currency: &str) -> Result<(f64, u16)> {
        let (rate_scaled, slippage_bps) = self.get_exchange_rate_scaled(currency)?;
        Ok((scaled_to_rate_f64(rate_scaled), slippage_bps))
    }
}

/// 可配置汇率提供器（主链路默认使用）
///
/// spec 格式：
/// `BTC:100000:80,ETH:5000:60,USDT:10:30`
/// 含义：`currency:rate:slippage_bps`
pub struct ConfigurableExchangeRateProvider {
    source_name: String,
    rates_scaled: HashMap<String, u128>,
    slippage_bps: HashMap<String, u16>,
}

impl ConfigurableExchangeRateProvider {
    pub fn deterministic_v1() -> Self {
        let mut rates_scaled = HashMap::new();
        let mut slippage_bps = HashMap::new();
        rates_scaled.insert("BTC".to_string(), 100_000 * FOREIGN_RATE_SCALE);
        rates_scaled.insert("ETH".to_string(), 5_000 * FOREIGN_RATE_SCALE);
        rates_scaled.insert("USDT".to_string(), 10 * FOREIGN_RATE_SCALE);
        slippage_bps.insert("BTC".to_string(), 80);
        slippage_bps.insert("ETH".to_string(), 60);
        slippage_bps.insert("USDT".to_string(), 30);
        Self {
            source_name: "deterministic_v1".to_string(),
            rates_scaled,
            slippage_bps,
        }
    }

    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    pub fn set_source_name(&mut self, source_name: &str) -> Result<()> {
        let name = source_name.trim();
        if name.is_empty() {
            return Err(anyhow::anyhow!("source_name cannot be empty"));
        }
        self.source_name = name.to_string();
        Ok(())
    }

    pub fn set_rate_with_slippage(
        &mut self,
        currency: &str,
        rate: f64,
        slippage_bp: u16,
    ) -> Result<()> {
        let key = currency.trim().to_ascii_uppercase();
        if key.is_empty() {
            return Err(anyhow::anyhow!("currency cannot be empty"));
        }
        let rate_scaled = rate_to_scaled_f64(rate)?;
        if slippage_bp > 10_000 {
            return Err(anyhow::anyhow!(
                "invalid slippage_bps for {}: {}",
                key,
                slippage_bp
            ));
        }
        self.rates_scaled.insert(key.clone(), rate_scaled);
        self.slippage_bps.insert(key, slippage_bp);
        Ok(())
    }

    pub fn set_rate(&mut self, currency: &str, rate: f64) -> Result<()> {
        let key = currency.trim().to_ascii_uppercase();
        let slippage = self.slippage_bps.get(&key).copied().unwrap_or(50);
        self.set_rate_with_slippage(currency, rate, slippage)
    }

    pub fn apply_quote_spec(&mut self, spec: &str) -> Result<()> {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            return Err(anyhow::anyhow!("quote spec cannot be empty"));
        }

        for entry in trimmed.split(',') {
            let raw = entry.trim();
            if raw.is_empty() {
                continue;
            }
            let mut parts = raw.split(':').map(|p| p.trim());
            let currency = parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("invalid quote entry: {}", raw))?;
            let rate_str = parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("invalid quote entry: {}", raw))?;
            let slippage_str = parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("invalid quote entry: {}", raw))?;
            if parts.next().is_some() {
                return Err(anyhow::anyhow!("invalid quote entry: {}", raw));
            }
            let rate_scaled = parse_rate_str_to_scaled(rate_str)?;
            let slippage = slippage_str
                .parse::<u16>()
                .map_err(|_| anyhow::anyhow!("invalid slippage in quote entry: {}", raw))?;
            let key = currency.trim().to_ascii_uppercase();
            if key.is_empty() {
                return Err(anyhow::anyhow!("currency cannot be empty"));
            }
            if slippage > 10_000 {
                return Err(anyhow::anyhow!(
                    "invalid slippage_bps for {}: {}",
                    key,
                    slippage
                ));
            }
            self.rates_scaled.insert(key.clone(), rate_scaled);
            self.slippage_bps.insert(key, slippage);
        }

        Ok(())
    }
}

impl ExchangeRateProvider for ConfigurableExchangeRateProvider {
    fn get_exchange_rate_scaled(&self, currency: &str) -> Result<(u128, u16)> {
        let key = currency.to_ascii_uppercase();
        let rate_scaled = self
            .rates_scaled
            .get(&key)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Unsupported currency: {}", currency))?;
        let slippage = self.slippage_bps.get(&key).copied().unwrap_or(50);
        Ok((rate_scaled, slippage))
    }
}

impl Default for ConfigurableExchangeRateProvider {
    fn default() -> Self {
        Self::deterministic_v1()
    }
}

/// Simple fixed-rate provider (test/placeholder)
pub struct MockExchangeRateProvider {
    rates_scaled: HashMap<String, u128>,
    slippage_bps: HashMap<String, u16>,
}

impl MockExchangeRateProvider {
    pub fn new() -> Self {
        let mut rates_scaled = HashMap::new();
        let mut slippage_bps = HashMap::new();
        // Placeholder rates: 1 BTC = 100,000 Token, 1 ETH = 5,000 Token, 1 USDT = 10 Token
        rates_scaled.insert("BTC".to_string(), 100_000 * FOREIGN_RATE_SCALE);
        rates_scaled.insert("ETH".to_string(), 5_000 * FOREIGN_RATE_SCALE);
        rates_scaled.insert("USDT".to_string(), 10 * FOREIGN_RATE_SCALE);
        slippage_bps.insert("BTC".to_string(), 80);
        slippage_bps.insert("ETH".to_string(), 60);
        slippage_bps.insert("USDT".to_string(), 30);
        Self {
            rates_scaled,
            slippage_bps,
        }
    }

    pub fn set_rate(&mut self, currency: &str, rate: f64) {
        if let Ok(rate_scaled) = rate_to_scaled_f64(rate) {
            self.rates_scaled
                .insert(currency.to_ascii_uppercase(), rate_scaled);
        }
    }

    pub fn set_rate_with_slippage(&mut self, currency: &str, rate: f64, slippage_bp: u16) {
        let key = currency.to_ascii_uppercase();
        if let Ok(rate_scaled) = rate_to_scaled_f64(rate) {
            self.rates_scaled.insert(key.clone(), rate_scaled);
            self.slippage_bps.insert(key, slippage_bp);
        }
    }
}

impl ExchangeRateProvider for MockExchangeRateProvider {
    fn get_exchange_rate_scaled(&self, currency: &str) -> Result<(u128, u16)> {
        let key = currency.to_ascii_uppercase();
        let rate_scaled = self
            .rates_scaled
            .get(&key)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Unsupported currency: {}", currency))?;
        let slippage = self.slippage_bps.get(&key).copied().unwrap_or(50);
        Ok((rate_scaled, slippage))
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
    const RATE_SCALE: u128 = FOREIGN_RATE_SCALE;

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

impl ForeignPaymentProcessorImpl<ConfigurableExchangeRateProvider> {
    /// Read current configured foreign rate source name.
    pub fn rate_source_name(&self) -> &str {
        self.rate_provider.source_name()
    }

    /// Set foreign rate source name for audit / policy trace.
    pub fn set_rate_source_name(&mut self, source_name: &str) -> Result<()> {
        self.rate_provider.set_source_name(source_name)
    }

    /// Apply quote spec in format: `BTC:120000:90,ETH:6000:70,USDT:10:20`.
    pub fn apply_quote_spec(&mut self, spec: &str) -> Result<()> {
        self.rate_provider.apply_quote_spec(spec)
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

        let payment_record = self.pay_miner_in_token(miner, token_amount, payment)?;
        let payment_record = MinerPayment {
            exchange_rate,
            ..payment_record
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
        let (rate_scaled, _slippage) = self
            .rate_provider
            .get_exchange_rate_scaled(currency)
            .map_err(|e| e.to_string())?;

        let token_amount = foreign_amount
            .saturating_mul(rate_scaled)
            .saturating_div(Self::RATE_SCALE);

        Ok((token_amount, scaled_to_rate_f64(rate_scaled)))
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
        let (rate_scaled, _slippage) = self
            .rate_provider
            .get_exchange_rate_scaled(target_currency)
            .map_err(|e| e.to_string())?;

        if rate_scaled == 0 {
            return Err("invalid rate: zero".to_string());
        }
        let foreign_amount = token_amount.saturating_mul(Self::RATE_SCALE) / rate_scaled;

        Ok((foreign_amount, scaled_to_rate_f64(rate_scaled)))
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

    fn build_configured_processor() -> ForeignPaymentProcessorImpl<ConfigurableExchangeRateProvider>
    {
        let rate_provider = ConfigurableExchangeRateProvider::deterministic_v1();
        ForeignPaymentProcessorImpl::new(ForeignPaymentConfig {
            rate_provider,
            treasury_address: addr(210),
            m0_pool_address: addr(211),
        })
    }

    #[test]
    fn test_configurable_rate_provider_apply_quote_spec_ok() {
        let mut provider = ConfigurableExchangeRateProvider::deterministic_v1();
        provider
            .set_source_name("configured_file_v1")
            .expect("set source");
        provider
            .apply_quote_spec("BTC:120000:90,ETH:6000:70,USDT:9.8:20")
            .expect("apply spec");
        let (btc_rate, btc_slippage) = provider.get_exchange_rate("BTC").expect("btc quote");
        let (eth_rate, eth_slippage) = provider.get_exchange_rate("ETH").expect("eth quote");
        let (usdt_rate, usdt_slippage) = provider.get_exchange_rate("USDT").expect("usdt quote");

        assert_eq!(provider.source_name(), "configured_file_v1");
        assert!((btc_rate - 120_000.0).abs() < 1e-9);
        assert_eq!(btc_slippage, 90);
        assert!((eth_rate - 6_000.0).abs() < 1e-9);
        assert_eq!(eth_slippage, 70);
        assert!((usdt_rate - 9.8).abs() < 1e-9);
        assert_eq!(usdt_slippage, 20);
    }

    #[test]
    fn test_configurable_rate_provider_reject_invalid_rate() {
        let mut provider = ConfigurableExchangeRateProvider::deterministic_v1();
        let err = provider
            .apply_quote_spec("BTC:0:50")
            .expect_err("rate=0 should be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("rate") || msg.contains("invalid"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn test_configurable_rate_provider_reject_invalid_slippage() {
        let mut provider = ConfigurableExchangeRateProvider::deterministic_v1();
        let err = provider
            .apply_quote_spec("BTC:100000:10001")
            .expect_err("slippage > 10000 should be rejected");
        assert!(err.to_string().contains("invalid slippage_bps"));
    }

    #[test]
    fn test_foreign_payment_processing_with_configured_provider() {
        let mut processor = build_configured_processor();
        let miner = addr(55);

        let payment = ForeignPayment {
            currency: "USDT".to_string(),
            amount: 2_000_000, // 2 USDT
            payer: "0xuser".to_string(),
            service_type: ServiceType::Gas,
        };

        let result = processor
            .process_foreign_payment(payment, miner)
            .expect("process");

        // deterministic_v1: 1 USDT = 10 token
        assert_eq!(result.token_amount, 20_000_000);
        assert!((result.exchange_rate - 10.0).abs() < 1e-9);
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
