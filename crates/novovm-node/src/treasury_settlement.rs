#![forbid(unsafe_code)]

use crate::clearing_types::NovClearingResultV1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NovTreasurySettlementInputV1 {
    pub tx_id: String,
    pub pay_asset: String,
    pub pay_amount: u128,
    pub settled_fee_nov: u128,
    pub route_id: String,
    pub route_source: String,
}

pub fn settle_clearing_result_into_treasury_v1(
    tx_id: impl Into<String>,
    clearing: &NovClearingResultV1,
) -> NovTreasurySettlementInputV1 {
    NovTreasurySettlementInputV1 {
        tx_id: tx_id.into(),
        pay_asset: clearing.pay_asset.clone(),
        pay_amount: clearing.pay_amount,
        settled_fee_nov: clearing.nov_amount_out,
        route_id: clearing.route_id.clone(),
        route_source: clearing.route_source.as_str().to_string(),
    }
}
