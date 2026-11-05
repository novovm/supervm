//! Explicit amount pricing and budget allocation; no market-price lookup or
//! provider payout. The original fee owner retains the collected NOV once.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeeRoundingV2 {
    Floor,
    Ceil,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitRemainderV2 {
    Service,
    Network,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProportionalFeeV3 {
    pub rate_bps: u32,
    pub rounding: FeeRoundingV2,
    /// Explicit base-unit bounds, applied after proportional rounding.
    pub minimum_fee_units: u128,
    pub maximum_fee_units: u128,
    pub service_share_bps: u32,
    pub network_share_bps: u32,
    pub split_remainder: SplitRemainderV2,
}

impl ProportionalFeeV3 {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=10_000).contains(&self.rate_bps),
            "explicit amount fee rate must be 1..=10000 bps"
        );
        ensure!(
            self.maximum_fee_units > 0 && self.minimum_fee_units <= self.maximum_fee_units,
            "amount fee requires 0 <= minimum <= positive maximum"
        );
        ensure!(
            u64::from(self.service_share_bps) + u64::from(self.network_share_bps) == 10_000,
            "service and network fee budgets must sum to 10000 bps"
        );
        Ok(())
    }

    pub(super) fn fee(&self, amount: u128) -> u128 {
        let (whole, remainder) = ratio_parts(amount, self.rate_bps);
        let rounded = whole + u128::from(self.rounding == FeeRoundingV2::Ceil && remainder != 0);
        rounded
            .max(self.minimum_fee_units)
            .min(self.maximum_fee_units)
    }

    pub(super) fn split(&self, amount: u128) -> [u128; 3] {
        // These are disjoint budget views in the EXISTING balance: reserve is
        // network purposes, fee is service obligations, risk is unused in v3.
        match self.split_remainder {
            SplitRemainderV2::Service => {
                let network = ratio_parts(amount, self.network_share_bps).0;
                [network, amount - network, 0]
            }
            SplitRemainderV2::Network => {
                let service = ratio_parts(amount, self.service_share_bps).0;
                [amount - service, service, 0]
            }
        }
    }
}

// rate <= denominator: quotient decomposition computes the exact product/divide
// without overflowing u128 even when the representable result fits at MAX.
fn ratio_parts(amount: u128, rate: u32) -> (u128, u128) {
    let residual = (amount % BPS) * u128::from(rate);
    (
        (amount / BPS) * u128::from(rate) + residual / BPS,
        residual % BPS,
    )
}
