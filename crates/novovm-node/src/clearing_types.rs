#![forbid(unsafe_code)]

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NovRouteSourceV1 {
    TreasuryDirect,
    AmmPool,
    StaticConfig,
}

impl NovRouteSourceV1 {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TreasuryDirect => "treasury_direct",
            Self::AmmPool => "amm_pool",
            Self::StaticConfig => "static_config",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovLiquiditySourceIdV1 {
    pub source: NovRouteSourceV1,
    pub pool_id: Option<String>,
    pub asset_in: String,
    pub asset_out: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovClearingRouteQuoteV1 {
    pub route_id: String,
    pub source_id: NovLiquiditySourceIdV1,
    pub pay_asset: String,
    pub settle_asset: String,
    pub pay_amount_in: u128,
    pub expected_nov_out: u128,
    pub fee_ppm: u32,
    pub quoted_at_ms: u64,
    pub expires_at_ms: u64,
    pub liquidity_available: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovSelectedClearingRouteV1 {
    pub route_quote: NovClearingRouteQuoteV1,
    pub selection_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovClearingResultV1 {
    pub route_id: String,
    pub route_source: NovRouteSourceV1,
    pub pay_asset: String,
    pub pay_amount: u128,
    pub nov_amount_out: u128,
    pub fee_ppm: u32,
    pub cleared_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NovClearingFailureCodeV1 {
    QuoteExpired,
    RouteUnavailable,
    InsufficientLiquidity,
    SlippageExceeded,
    MaxPayExceeded,
    ConstrainedRouteRestricted,
    ConstrainedDailyVolumeExceeded,
    ConstrainedBlocked,
    ClearingDisabled,
    DailyVolumeExceeded,
    RiskBufferBelowMin,
}

impl NovClearingFailureCodeV1 {
    pub fn as_error_code(&self) -> &'static str {
        match self {
            Self::QuoteExpired => "fee.clearing.quote_expired",
            Self::RouteUnavailable => "fee.clearing.route_unavailable",
            Self::InsufficientLiquidity => "fee.clearing.insufficient_liquidity",
            Self::SlippageExceeded => "fee.clearing.slippage_exceeded",
            Self::MaxPayExceeded => "fee.clearing.max_pay_exceeded",
            Self::ConstrainedRouteRestricted => "fee.clearing.constrained_route_restricted",
            Self::ConstrainedDailyVolumeExceeded => {
                "fee.clearing.constrained_daily_volume_exceeded"
            }
            Self::ConstrainedBlocked => "fee.clearing.constrained_blocked",
            Self::ClearingDisabled => "fee.clearing.clearing_disabled",
            Self::DailyVolumeExceeded => "fee.clearing.daily_volume_exceeded",
            Self::RiskBufferBelowMin => "fee.clearing.risk_buffer_below_min",
        }
    }

    pub fn short_reason(&self) -> &'static str {
        match self {
            Self::QuoteExpired => "quote_expired",
            Self::RouteUnavailable => "route_unavailable",
            Self::InsufficientLiquidity => "insufficient_liquidity",
            Self::SlippageExceeded => "slippage_exceeded",
            Self::MaxPayExceeded => "max_pay_exceeded",
            Self::ConstrainedRouteRestricted => "constrained_route_restricted",
            Self::ConstrainedDailyVolumeExceeded => "constrained_daily_volume_exceeded",
            Self::ConstrainedBlocked => "constrained_blocked",
            Self::ClearingDisabled => "clearing_disabled",
            Self::DailyVolumeExceeded => "daily_volume_exceeded",
            Self::RiskBufferBelowMin => "risk_buffer_below_min",
        }
    }
}

impl fmt::Display for NovClearingFailureCodeV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_error_code())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovExecutionFeeRequestV1 {
    pub tx_id: String,
    pub pay_asset: String,
    pub max_pay_amount: u128,
    pub nov_needed: u128,
    pub slippage_bps: u32,
    pub quote_required_pay_amount: u128,
    pub quote_with_slippage_pay_amount: u128,
    pub quote_expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovReceiptRouteMetaV1 {
    pub route_id: String,
    pub route_source: String,
    pub expected_nov_out: u128,
    pub route_fee_ppm: u32,
    #[serde(default)]
    pub selection_reason: String,
    #[serde(default)]
    pub candidate_route_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovStaticAmmPoolStateV1 {
    pub pool_id: String,
    pub asset_x: String,
    pub asset_y: String,
    pub reserve_x: u128,
    pub reserve_y: u128,
    pub swap_fee_ppm: u32,
    #[serde(default = "default_enabled_v1")]
    pub enabled: bool,
}

const fn default_enabled_v1() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovLastClearingRouteV1 {
    pub route_id: String,
    pub route_source: String,
    pub pay_asset: String,
    pub pay_amount: u128,
    pub nov_amount_out: u128,
    pub expected_nov_out: u128,
    pub route_fee_ppm: u32,
    pub cleared_at_ms: u64,
    #[serde(default)]
    pub selection_reason: String,
    #[serde(default)]
    pub candidate_route_count: u32,
}
