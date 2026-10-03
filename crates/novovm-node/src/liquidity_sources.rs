#![forbid(unsafe_code)]

use crate::clearing_types::{
    NovClearingFailureCodeV1, NovClearingResultV1, NovClearingRouteQuoteV1,
    NovExecutionFeeRequestV1, NovLiquiditySourceIdV1, NovRouteSourceV1,
};

const RATE_DENOMINATOR_PPM_V1: u128 = 1_000_000;

pub trait NovLiquiditySourceV1 {
    fn source_id(&self) -> NovLiquiditySourceIdV1;

    fn quote(
        &self,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Option<NovClearingRouteQuoteV1>;

    fn execute(
        &mut self,
        quote: &NovClearingRouteQuoteV1,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Result<NovClearingResultV1, NovClearingFailureCodeV1>;
}

#[derive(Debug, Clone)]
pub struct TreasuryDirectLiquidityV1 {
    pub asset: String,
    pub available_liquidity_nov: u128,
    pub clearing_rate_ppm: u128,
    pub quote_ttl_ms: u64,
}

impl TreasuryDirectLiquidityV1 {
    fn required_pay_amount_v1(&self, nov_needed: u128) -> Option<u128> {
        if self.clearing_rate_ppm == 0 {
            return None;
        }
        Some(
            nov_needed
                .saturating_mul(RATE_DENOMINATOR_PPM_V1)
                .saturating_add(self.clearing_rate_ppm.saturating_sub(1))
                / self.clearing_rate_ppm,
        )
    }

    fn expected_nov_out_for_input_v1(&self, pay_amount_in: u128) -> u128 {
        pay_amount_in
            .saturating_mul(self.clearing_rate_ppm)
            .saturating_div(RATE_DENOMINATOR_PPM_V1)
    }
}

impl NovLiquiditySourceV1 for TreasuryDirectLiquidityV1 {
    fn source_id(&self) -> NovLiquiditySourceIdV1 {
        NovLiquiditySourceIdV1 {
            source: NovRouteSourceV1::TreasuryDirect,
            pool_id: None,
            asset_in: self.asset.clone(),
            asset_out: "NOV".to_string(),
        }
    }

    fn quote(
        &self,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Option<NovClearingRouteQuoteV1> {
        if request.pay_asset != self.asset {
            return None;
        }
        let pay_amount_in = self.required_pay_amount_v1(request.nov_needed)?;
        let expected_nov_out = self.expected_nov_out_for_input_v1(request.max_pay_amount);
        Some(NovClearingRouteQuoteV1 {
            route_id: format!(
                "route:treasury_direct:{}:nov",
                self.asset.to_ascii_lowercase()
            ),
            source_id: self.source_id(),
            pay_asset: request.pay_asset.clone(),
            settle_asset: "NOV".to_string(),
            pay_amount_in,
            expected_nov_out: expected_nov_out.max(request.nov_needed),
            fee_ppm: 0,
            quoted_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(self.quote_ttl_ms.max(1)),
            liquidity_available: self.available_liquidity_nov,
        })
    }

    fn execute(
        &mut self,
        quote: &NovClearingRouteQuoteV1,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Result<NovClearingResultV1, NovClearingFailureCodeV1> {
        if now_ms > quote.expires_at_ms || now_ms > request.quote_expires_at_ms {
            return Err(NovClearingFailureCodeV1::QuoteExpired);
        }
        if quote.pay_amount_in > request.quote_with_slippage_pay_amount {
            return Err(NovClearingFailureCodeV1::SlippageExceeded);
        }
        if quote.pay_amount_in > request.max_pay_amount {
            return Err(NovClearingFailureCodeV1::MaxPayExceeded);
        }
        if self.available_liquidity_nov < request.nov_needed {
            return Err(NovClearingFailureCodeV1::InsufficientLiquidity);
        }
        self.available_liquidity_nov = self
            .available_liquidity_nov
            .saturating_sub(request.nov_needed);
        Ok(NovClearingResultV1 {
            route_id: quote.route_id.clone(),
            route_source: NovRouteSourceV1::TreasuryDirect,
            pay_asset: quote.pay_asset.clone(),
            pay_amount: quote.pay_amount_in,
            nov_amount_out: request.nov_needed,
            fee_ppm: quote.fee_ppm,
            cleared_at_ms: now_ms,
        })
    }
}

#[derive(Debug, Clone)]
pub struct StaticAmmPoolLiquidityV1 {
    pub pool_id: String,
    pub asset_x: String,
    pub asset_y: String,
    pub reserve_x: u128,
    pub reserve_y: u128,
    pub swap_fee_ppm: u32,
    pub quote_ttl_ms: u64,
}

impl StaticAmmPoolLiquidityV1 {
    fn required_input_for_exact_output_v1(
        reserve_in: u128,
        reserve_out: u128,
        amount_out: u128,
        fee_ppm: u32,
    ) -> Option<u128> {
        if reserve_in == 0 || reserve_out == 0 || amount_out == 0 || amount_out >= reserve_out {
            return None;
        }
        let fee_den = RATE_DENOMINATOR_PPM_V1;
        let fee_factor = fee_den.saturating_sub(fee_ppm as u128);
        if fee_factor == 0 {
            return None;
        }
        let numerator = reserve_in
            .saturating_mul(amount_out)
            .saturating_mul(fee_den);
        let denominator = reserve_out
            .saturating_sub(amount_out)
            .saturating_mul(fee_factor);
        if denominator == 0 {
            return None;
        }
        Some((numerator.saturating_add(denominator.saturating_sub(1))) / denominator)
    }

    fn output_for_exact_input_v1(
        reserve_in: u128,
        reserve_out: u128,
        amount_in: u128,
        fee_ppm: u32,
    ) -> Option<u128> {
        if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
            return None;
        }
        let fee_den = RATE_DENOMINATOR_PPM_V1;
        let amount_in_after_fee =
            amount_in.saturating_mul(fee_den.saturating_sub(fee_ppm as u128)) / fee_den;
        if amount_in_after_fee == 0 {
            return None;
        }
        let numerator = amount_in_after_fee.saturating_mul(reserve_out);
        let denominator = reserve_in.saturating_add(amount_in_after_fee);
        if denominator == 0 {
            return None;
        }
        Some(numerator / denominator)
    }
}

impl NovLiquiditySourceV1 for StaticAmmPoolLiquidityV1 {
    fn source_id(&self) -> NovLiquiditySourceIdV1 {
        NovLiquiditySourceIdV1 {
            source: NovRouteSourceV1::AmmPool,
            pool_id: Some(self.pool_id.clone()),
            asset_in: self.asset_x.clone(),
            asset_out: self.asset_y.clone(),
        }
    }

    fn quote(
        &self,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Option<NovClearingRouteQuoteV1> {
        if request.pay_asset != self.asset_x || self.asset_y != "NOV" {
            return None;
        }
        let pay_amount_in = Self::required_input_for_exact_output_v1(
            self.reserve_x,
            self.reserve_y,
            request.nov_needed,
            self.swap_fee_ppm,
        )?;
        let expected_nov_out = Self::output_for_exact_input_v1(
            self.reserve_x,
            self.reserve_y,
            request.max_pay_amount,
            self.swap_fee_ppm,
        )
        .unwrap_or(0);
        Some(NovClearingRouteQuoteV1 {
            route_id: format!(
                "route:amm_pool:{}:{}->{}",
                self.pool_id, self.asset_x, self.asset_y
            ),
            source_id: self.source_id(),
            pay_asset: request.pay_asset.clone(),
            settle_asset: "NOV".to_string(),
            pay_amount_in,
            expected_nov_out: expected_nov_out.max(request.nov_needed),
            fee_ppm: self.swap_fee_ppm,
            quoted_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(self.quote_ttl_ms.max(1)),
            liquidity_available: self.reserve_y,
        })
    }

    fn execute(
        &mut self,
        quote: &NovClearingRouteQuoteV1,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Result<NovClearingResultV1, NovClearingFailureCodeV1> {
        if now_ms > quote.expires_at_ms || now_ms > request.quote_expires_at_ms {
            return Err(NovClearingFailureCodeV1::QuoteExpired);
        }
        if quote.pay_amount_in > request.quote_with_slippage_pay_amount {
            return Err(NovClearingFailureCodeV1::SlippageExceeded);
        }
        if quote.pay_amount_in > request.max_pay_amount {
            return Err(NovClearingFailureCodeV1::MaxPayExceeded);
        }
        if self.reserve_y <= request.nov_needed {
            return Err(NovClearingFailureCodeV1::InsufficientLiquidity);
        }

        let mut pay_amount = quote.pay_amount_in;
        let actual_out = Self::output_for_exact_input_v1(
            self.reserve_x,
            self.reserve_y,
            pay_amount,
            self.swap_fee_ppm,
        )
        .ok_or(NovClearingFailureCodeV1::RouteUnavailable)?;
        if actual_out < request.nov_needed {
            let fallback_pay_amount = pay_amount.saturating_add(1);
            if fallback_pay_amount > request.quote_with_slippage_pay_amount
                || fallback_pay_amount > request.max_pay_amount
            {
                return Err(NovClearingFailureCodeV1::SlippageExceeded);
            }
            let fallback_out = Self::output_for_exact_input_v1(
                self.reserve_x,
                self.reserve_y,
                fallback_pay_amount,
                self.swap_fee_ppm,
            )
            .ok_or(NovClearingFailureCodeV1::RouteUnavailable)?;
            if fallback_out < request.nov_needed {
                return Err(NovClearingFailureCodeV1::SlippageExceeded);
            }
            pay_amount = fallback_pay_amount;
        }
        if request.nov_needed > self.reserve_y {
            return Err(NovClearingFailureCodeV1::InsufficientLiquidity);
        }

        self.reserve_x = self.reserve_x.saturating_add(pay_amount);
        self.reserve_y = self.reserve_y.saturating_sub(request.nov_needed);

        Ok(NovClearingResultV1 {
            route_id: quote.route_id.clone(),
            route_source: NovRouteSourceV1::AmmPool,
            pay_asset: quote.pay_asset.clone(),
            pay_amount,
            nov_amount_out: request.nov_needed,
            fee_ppm: quote.fee_ppm,
            cleared_at_ms: now_ms,
        })
    }
}
