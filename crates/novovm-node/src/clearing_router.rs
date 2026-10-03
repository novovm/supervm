#![forbid(unsafe_code)]

use crate::clearing_types::{
    NovClearingFailureCodeV1, NovClearingResultV1, NovClearingRouteQuoteV1,
    NovExecutionFeeRequestV1, NovSelectedClearingRouteV1,
};
use crate::liquidity_sources::NovLiquiditySourceV1;

pub trait NovClearingRouterV1 {
    fn quote_routes(
        &self,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Vec<NovClearingRouteQuoteV1>;

    fn select_best_route(
        &self,
        routes: &[NovClearingRouteQuoteV1],
    ) -> Result<NovSelectedClearingRouteV1, NovClearingFailureCodeV1>;

    fn execute_selected_route(
        &mut self,
        selected: &NovSelectedClearingRouteV1,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Result<NovClearingResultV1, NovClearingFailureCodeV1>;
}

pub struct NovClearingRouterImplV1 {
    pub sources: Vec<Box<dyn NovLiquiditySourceV1>>,
}

const ROUTE_SELECTION_REASON_MULTI_CONDITION_V1: &str =
    "expected_out_then_liquidity_then_freshness";

impl NovClearingRouterImplV1 {
    pub fn new(sources: Vec<Box<dyn NovLiquiditySourceV1>>) -> Self {
        Self { sources }
    }

    fn selection_key_v1(route: &NovClearingRouteQuoteV1) -> (u128, u128, u64, u64, &str) {
        (
            route.expected_nov_out,
            route.liquidity_available,
            route.quoted_at_ms,
            route.expires_at_ms,
            route.route_id.as_str(),
        )
    }
}

impl NovClearingRouterV1 for NovClearingRouterImplV1 {
    fn quote_routes(
        &self,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Vec<NovClearingRouteQuoteV1> {
        self.sources
            .iter()
            .filter_map(|source| source.quote(request, now_ms))
            .collect()
    }

    fn select_best_route(
        &self,
        routes: &[NovClearingRouteQuoteV1],
    ) -> Result<NovSelectedClearingRouteV1, NovClearingFailureCodeV1> {
        let best = routes
            .iter()
            .max_by_key(|quote| Self::selection_key_v1(quote))
            .cloned()
            .ok_or(NovClearingFailureCodeV1::RouteUnavailable)?;
        Ok(NovSelectedClearingRouteV1 {
            route_quote: best,
            selection_reason: ROUTE_SELECTION_REASON_MULTI_CONDITION_V1.to_string(),
        })
    }

    fn execute_selected_route(
        &mut self,
        selected: &NovSelectedClearingRouteV1,
        request: &NovExecutionFeeRequestV1,
        now_ms: u64,
    ) -> Result<NovClearingResultV1, NovClearingFailureCodeV1> {
        for source in &mut self.sources {
            let source_id = source.source_id();
            if source_id == selected.route_quote.source_id {
                return source.execute(&selected.route_quote, request, now_ms);
            }
        }
        Err(NovClearingFailureCodeV1::RouteUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clearing_types::{NovLiquiditySourceIdV1, NovRouteSourceV1};

    fn make_quote(
        route_id: &str,
        expected: u128,
        liquidity: u128,
        quoted_at: u64,
    ) -> NovClearingRouteQuoteV1 {
        NovClearingRouteQuoteV1 {
            route_id: route_id.to_string(),
            source_id: NovLiquiditySourceIdV1 {
                source: NovRouteSourceV1::AmmPool,
                pool_id: Some(route_id.to_string()),
                asset_in: "USDT".to_string(),
                asset_out: "NOV".to_string(),
            },
            pay_asset: "USDT".to_string(),
            settle_asset: "NOV".to_string(),
            pay_amount_in: 10,
            expected_nov_out: expected,
            fee_ppm: 3_000,
            quoted_at_ms: quoted_at,
            expires_at_ms: quoted_at.saturating_add(15_000),
            liquidity_available: liquidity,
        }
    }

    #[test]
    fn select_best_route_prefers_expected_then_liquidity_then_freshness() {
        let router = NovClearingRouterImplV1::new(Vec::new());
        let routes = vec![
            make_quote("r-a", 100, 200, 1000),
            make_quote("r-b", 100, 300, 900),
            make_quote("r-c", 100, 300, 1100),
        ];
        let selected = router
            .select_best_route(&routes)
            .expect("selection should succeed");
        assert_eq!(selected.route_quote.route_id, "r-c");
        assert_eq!(
            selected.selection_reason,
            "expected_out_then_liquidity_then_freshness"
        );
    }

    #[test]
    fn select_best_route_prefers_higher_expected_out_first() {
        let router = NovClearingRouterImplV1::new(Vec::new());
        let routes = vec![
            make_quote("r-a", 101, 50, 1000),
            make_quote("r-b", 100, 10_000, 2000),
        ];
        let selected = router
            .select_best_route(&routes)
            .expect("selection should succeed");
        assert_eq!(selected.route_quote.route_id, "r-a");
    }
}
