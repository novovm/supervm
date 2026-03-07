//! End-to-end integration: full fee routing flow
//!
//! Test scenarios:
//! 1. User pays gas → MainnetToken split → node pool/treasury/burn
//! 2. Treasury income → Treasury income event
//! 3. Treasury allocates portion to DividendPool
//! 4. User actively claims dividends

use crate::{
    dividend_pool::{DividendEvent, DividendPool, DividendPoolImpl},
    mainnet_token::{FeeSplit, MainnetToken, MainnetTokenEvent},
    mainnet_token_impl::{MainnetTokenConfig, MainnetTokenImpl},
    treasury::{Treasury, TreasuryAccountKind, TreasuryEvent},
    treasury_impl::{BuybackConfig, NavConfig, TreasuryConfig, TreasuryImpl},
    types::Address,
};
use std::collections::HashMap;

/// E2E test harness
/// Simulates interactions among Token/Treasury/DividendPool
pub struct E2ETestHarness {
    pub token: MainnetTokenImpl,
    pub treasury: TreasuryImpl,
    pub dividend_pool: DividendPoolImpl,
}

impl E2ETestHarness {
    /// Create test environment
    pub fn new() -> Self {
        let treasury_addr = addr(200);
        let node_pool = addr(201);
        let service_pool = addr(202);

        // 1. Initialize Token
        let fee_split = FeeSplit {
            gas_base_burn_bp: 2000,       // 20% burn
            gas_to_node_bp: 3000,         // 30% to node
            service_burn_bp: 1000,        // 10% burn
            service_to_provider_bp: 4000, // 40% to provider
        };

        let initial_allocations = vec![
            (addr(1), 10_000_000_000), // user1: 10 billion
            (addr(2), 5_000_000_000),  // user2: 5 billion
            (addr(3), 3_000_000_000),  // user3: 3 billion
        ];

        let token = MainnetTokenImpl::new(MainnetTokenConfig {
            name: "SuperVM".into(),
            symbol: "SVM".into(),
            decimals: 9,
            max_supply: 100_000_000_000_000, // 1000 billion
            initial_allocations,
            locked_supply: 50_000_000_000_000, // 500 billion locked
            fee_split,
            treasury_account: treasury_addr,
            node_reward_pool: node_pool,
            service_provider_pool: service_pool,
            unlock_controller: addr(250),
        })
        .expect("token init");

        // 2. Initialize Treasury
        let mut treasury_balances = HashMap::new();
        treasury_balances.insert(TreasuryAccountKind::Main, 0);
        treasury_balances.insert(TreasuryAccountKind::Ecosystem, 0);
        treasury_balances.insert(TreasuryAccountKind::RiskReserve, 0);

        let treasury = TreasuryImpl::new(TreasuryConfig {
            initial_balances: treasury_balances,
            controller: addr(100),
            buyback_config: BuybackConfig::default(),
            nav_config: NavConfig::default(),
        });

        // 3. Initialize DividendPool
        let dividend_pool = DividendPoolImpl::new(100); // minimum holding 100

        Self {
            token,
            treasury,
            dividend_pool,
        }
    }

    /// Simulate gas fee payment flow
    pub fn simulate_gas_payment(
        &mut self,
        payer: &Address,
        amount: u128,
    ) -> (MainnetTokenEvent, TreasuryEvent) {
        // 1. MainnetToken splits fees
        let token_event = self
            .token
            .on_gas_fee_paid(payer, amount)
            .expect("gas fee routing");

        // 2. Extract treasury portion and credit
        let treasury_amount =
            if let MainnetTokenEvent::GasFeeRouted { to_treasury, .. } = &token_event {
                *to_treasury
            } else {
                panic!("unexpected event");
            };

        let treasury_addr = self.token.treasury_account();
        let treasury_event = self
            .treasury
            .on_income(&treasury_addr, treasury_amount, TreasuryAccountKind::Main)
            .expect("treasury income");

        (token_event, treasury_event)
    }

    /// Simulate service fee payment flow
    pub fn simulate_service_fee(
        &mut self,
        payer: &Address,
        service_id: [u8; 32],
        amount: u128,
    ) -> (MainnetTokenEvent, TreasuryEvent) {
        let token_event = self
            .token
            .on_service_fee_paid(service_id, payer, amount)
            .expect("service fee routing");

        let treasury_amount =
            if let MainnetTokenEvent::ServiceFeeRouted { to_treasury, .. } = &token_event {
                *to_treasury
            } else {
                panic!("unexpected event");
            };

        let treasury_addr = self.token.treasury_account();
        let treasury_event = self
            .treasury
            .on_income(&treasury_addr, treasury_amount, TreasuryAccountKind::Main)
            .expect("treasury income");

        (token_event, treasury_event)
    }

    /// Simulate treasury allocation to dividend pool
    pub fn treasury_to_dividend_pool(&mut self, amount: u128) -> DividendEvent {
        let treasury_addr = addr(200);
        self.dividend_pool
            .receive_income(&treasury_addr, amount)
            .expect("dividend income")
    }
}

impl Default for E2ETestHarness {
    fn default() -> Self {
        Self::new()
    }
}

fn addr(id: u8) -> Address {
    Address::from_bytes([id; 32])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_e2e_gas_fee_routing() {
        let mut harness = E2ETestHarness::new();
        let payer = addr(1);

        // User1 pays 10,000,000 gas
        let initial_balance = harness.token.balance_of(&payer);
        let (token_event, treasury_event) = harness.simulate_gas_payment(&payer, 10_000_000);

        // 验证 Token 事件
        if let MainnetTokenEvent::GasFeeRouted {
            to_node,
            to_treasury,
            to_burn,
            ..
        } = token_event
        {
            // 30% to node, 20% burn, 50% to treasury
            assert_eq!(to_node, 3_000_000);
            assert_eq!(to_burn, 2_000_000);
            assert_eq!(to_treasury, 5_000_000);
        } else {
            panic!("unexpected token event");
        }

        // Verify treasury income
        if let TreasuryEvent::Income { amount, .. } = treasury_event {
            assert_eq!(amount, 5_000_000);
        } else {
            panic!("unexpected treasury event");
        }

        // Verify balances
        assert_eq!(
            harness.token.balance_of(&payer),
            initial_balance - 10_000_000
        );
        assert_eq!(
            harness.treasury.balance_of(TreasuryAccountKind::Main),
            5_000_000
        );
        assert_eq!(harness.token.balance_of(&addr(201)), 3_000_000); // node pool
    }

    #[test]
    fn test_e2e_service_fee_routing() {
        let mut harness = E2ETestHarness::new();
        let payer = addr(2);

        // User2 pays 20,000,000 service fee
        let (token_event, _treasury_event) =
            harness.simulate_service_fee(&payer, [1u8; 32], 20_000_000);

        // Verify service fee split: 40% provider, 10% burn, 50% treasury
        if let MainnetTokenEvent::ServiceFeeRouted {
            to_provider,
            to_treasury,
            to_burn,
            ..
        } = token_event
        {
            assert_eq!(to_provider, 8_000_000);
            assert_eq!(to_burn, 2_000_000);
            assert_eq!(to_treasury, 10_000_000);
        } else {
            panic!("unexpected event");
        }

        assert_eq!(
            harness.treasury.balance_of(TreasuryAccountKind::Main),
            10_000_000
        );
        assert_eq!(harness.token.balance_of(&addr(202)), 8_000_000); // provider pool
    }

    #[test]
    fn test_e2e_treasury_to_dividend() {
        let mut harness = E2ETestHarness::new();
        let payer = addr(1);

        // 1. User pays gas → treasury income
        harness.simulate_gas_payment(&payer, 100_000_000);
        let treasury_balance = harness.treasury.balance_of(TreasuryAccountKind::Main);
        assert_eq!(treasury_balance, 50_000_000); // 50%

        // 2. Treasury allocates 80% to dividend pool
        let dividend_amount = treasury_balance * 80 / 100;
        let event = harness.treasury_to_dividend_pool(dividend_amount);

        if let DividendEvent::IncomeReceived { amount, .. } = event {
            assert_eq!(amount, 40_000_000);
        } else {
            panic!("unexpected dividend event");
        }

        assert_eq!(harness.dividend_pool.pool_balance(), 40_000_000);
    }

    #[test]
    fn test_e2e_multiple_fees_accumulation() {
        let mut harness = E2ETestHarness::new();

        // Simulate multiple gas fees
        harness.simulate_gas_payment(&addr(1), 10_000_000);
        harness.simulate_gas_payment(&addr(2), 20_000_000);
        harness.simulate_service_fee(&addr(3), [2u8; 32], 30_000_000);

        // Treasury cumulative income = 5M + 10M + 15M = 30M
        let total_treasury = harness.treasury.balance_of(TreasuryAccountKind::Main);
        assert_eq!(total_treasury, 30_000_000);

        // Node pool = 3M + 6M = 9M
        assert_eq!(harness.token.balance_of(&addr(201)), 9_000_000);

        // Provider pool = 12M
        assert_eq!(harness.token.balance_of(&addr(202)), 12_000_000);
    }

    #[test]
    fn test_e2e_burn_tracking() {
        let mut harness = E2ETestHarness::new();
        let initial_supply = harness.token.total_supply();

        // Multiple transactions cause burns
        harness.simulate_gas_payment(&addr(1), 10_000_000); // burn 2M
        harness.simulate_service_fee(&addr(2), [3u8; 32], 20_000_000); // burn 2M

        // Total supply should decrease by 4M
        assert_eq!(harness.token.total_supply(), initial_supply - 4_000_000);
    }
}
