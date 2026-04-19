use crate::types::{BFTError, BFTResult, FeeRoutingOutcome, NodeId, TokenEconomicsPolicy};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use web30_core::mainnet_token::{FeeSplit as Web30FeeSplit, MainnetToken, MainnetTokenEvent};
use web30_core::mainnet_token_impl::{MainnetTokenConfig, MainnetTokenImpl};
use web30_core::types::Address as Web30Address;

const ADDR_DOMAIN_NODE: u8 = 0xA0;
const ADDR_DOMAIN_SYSTEM: u8 = 0xB0;
const SYS_ADDR_TREASURY: u8 = 200;
const SYS_ADDR_NODE_POOL: u8 = 201;
const SYS_ADDR_SERVICE_POOL: u8 = 202;
const SYS_ADDR_UNLOCK_CTRL: u8 = 250;

fn node_to_address(node: NodeId) -> Web30Address {
    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&node.to_le_bytes());
    bytes[31] = ADDR_DOMAIN_NODE;
    Web30Address::from_bytes(bytes)
}

fn system_address(tag: u8) -> Web30Address {
    let mut bytes = [0u8; 32];
    bytes[0] = tag;
    bytes[31] = ADDR_DOMAIN_SYSTEM;
    Web30Address::from_bytes(bytes)
}

fn to_u64(value: u128, ctx: &str) -> BFTResult<u64> {
    u64::try_from(value).map_err(|_| BFTError::Internal(format!("{} out of u64 range", ctx)))
}

fn from_web30_error(ctx: &str, err: impl std::fmt::Display) -> BFTError {
    BFTError::InvalidProposal(format!("{}: {}", ctx, err))
}

fn to_web30_split(policy: &TokenEconomicsPolicy) -> Web30FeeSplit {
    Web30FeeSplit {
        gas_base_burn_bp: policy.fee_split.gas_base_burn_bp,
        gas_to_node_bp: policy.fee_split.gas_to_node_bp,
        service_burn_bp: policy.fee_split.service_burn_bp,
        service_to_provider_bp: policy.fee_split.service_to_provider_bp,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Web30TrackedBalanceSnapshot {
    pub address_hex: String,
    pub balance: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Web30TokenRuntimeSnapshot {
    pub minted_locked_total: u64,
    pub burned_total: u64,
    pub treasury_spent_total: u64,
    pub tracked_balances: Vec<Web30TrackedBalanceSnapshot>,
}

fn address_to_hex(address: &Web30Address) -> String {
    let mut out = String::with_capacity(address.as_bytes().len() * 2);
    for byte in address.as_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn address_from_hex(raw: &str) -> BFTResult<Web30Address> {
    let normalized = raw
        .trim()
        .strip_prefix("0x")
        .or_else(|| raw.trim().strip_prefix("0X"))
        .unwrap_or(raw.trim());
    if normalized.len() != 64 {
        return Err(BFTError::Internal(format!(
            "token runtime address hex must be 64 chars, got {}",
            normalized.len()
        )));
    }
    let mut bytes = [0u8; 32];
    for (idx, pair) in normalized.as_bytes().chunks_exact(2).enumerate() {
        let hex = std::str::from_utf8(pair).map_err(|_| {
            BFTError::Internal("token runtime address hex contains invalid utf8".to_string())
        })?;
        bytes[idx] = u8::from_str_radix(hex, 16).map_err(|_| {
            BFTError::Internal(format!(
                "token runtime address hex contains invalid byte {}",
                hex
            ))
        })?;
    }
    Ok(Web30Address::from_bytes(bytes))
}

pub struct Web30TokenRuntime {
    token: MainnetTokenImpl,
    minted_locked_total: u64,
    burned_total: u64,
    treasury_spent_total: u64,
    tracked_accounts: HashSet<Web30Address>,
    treasury_account: Web30Address,
    node_reward_pool: Web30Address,
    service_provider_pool: Web30Address,
    unlock_controller: Web30Address,
}

impl Web30TokenRuntime {
    pub fn from_policy(policy: &TokenEconomicsPolicy) -> BFTResult<Self> {
        policy.validate()?;
        let treasury_account = system_address(SYS_ADDR_TREASURY);
        let node_reward_pool = system_address(SYS_ADDR_NODE_POOL);
        let service_provider_pool = system_address(SYS_ADDR_SERVICE_POOL);
        let unlock_controller = system_address(SYS_ADDR_UNLOCK_CTRL);
        let token = MainnetTokenImpl::new(MainnetTokenConfig {
            name: "NOVOVM".to_string(),
            symbol: "NOV".to_string(),
            decimals: 9,
            max_supply: policy.max_supply as u128,
            initial_allocations: vec![],
            locked_supply: policy.locked_supply as u128,
            fee_split: to_web30_split(policy),
            treasury_account,
            node_reward_pool,
            service_provider_pool,
            unlock_controller,
        })
        .map_err(|e| from_web30_error("init web30 mainnet token", e))?;

        let mut tracked_accounts = HashSet::new();
        tracked_accounts.insert(treasury_account);
        tracked_accounts.insert(node_reward_pool);
        tracked_accounts.insert(service_provider_pool);

        Ok(Self {
            token,
            minted_locked_total: 0,
            burned_total: 0,
            treasury_spent_total: 0,
            tracked_accounts,
            treasury_account,
            node_reward_pool,
            service_provider_pool,
            unlock_controller,
        })
    }

    pub fn snapshot(&self) -> Web30TokenRuntimeSnapshot {
        let mut tracked_balances: Vec<_> = self
            .tracked_accounts
            .iter()
            .copied()
            .map(|account| Web30TrackedBalanceSnapshot {
                address_hex: address_to_hex(&account),
                balance: self.token.balance_of(&account),
            })
            .collect();
        tracked_balances.sort_by(|left, right| left.address_hex.cmp(&right.address_hex));
        Web30TokenRuntimeSnapshot {
            minted_locked_total: self.minted_locked_total,
            burned_total: self.burned_total,
            treasury_spent_total: self.treasury_spent_total,
            tracked_balances,
        }
    }

    pub fn restore_from_snapshot(
        policy: &TokenEconomicsPolicy,
        snapshot: &Web30TokenRuntimeSnapshot,
    ) -> BFTResult<Self> {
        policy.validate()?;
        if snapshot.minted_locked_total > policy.locked_supply {
            return Err(BFTError::InvalidProposal(format!(
                "token runtime snapshot minted_locked_total {} exceeds locked_supply {}",
                snapshot.minted_locked_total, policy.locked_supply
            )));
        }
        let treasury_account = system_address(SYS_ADDR_TREASURY);
        let node_reward_pool = system_address(SYS_ADDR_NODE_POOL);
        let service_provider_pool = system_address(SYS_ADDR_SERVICE_POOL);
        let unlock_controller = system_address(SYS_ADDR_UNLOCK_CTRL);
        let mut tracked_accounts = HashSet::new();
        tracked_accounts.insert(treasury_account);
        tracked_accounts.insert(node_reward_pool);
        tracked_accounts.insert(service_provider_pool);

        let mut initial_allocations = Vec::with_capacity(snapshot.tracked_balances.len());
        let mut total_supply = 0u128;
        for item in &snapshot.tracked_balances {
            let address = address_from_hex(&item.address_hex)?;
            tracked_accounts.insert(address);
            if item.balance > 0 {
                total_supply = total_supply.checked_add(item.balance).ok_or_else(|| {
                    BFTError::Internal("token runtime snapshot total_supply overflow".to_string())
                })?;
                initial_allocations.push((address, item.balance));
            }
        }
        if total_supply > u128::from(policy.max_supply) {
            return Err(BFTError::InvalidProposal(format!(
                "token runtime snapshot total_supply {} exceeds max_supply {}",
                total_supply, policy.max_supply
            )));
        }

        let token = MainnetTokenImpl::new(MainnetTokenConfig {
            name: "NOVOVM".to_string(),
            symbol: "NOV".to_string(),
            decimals: 9,
            max_supply: policy.max_supply as u128,
            initial_allocations,
            locked_supply: policy
                .locked_supply
                .saturating_sub(snapshot.minted_locked_total) as u128,
            fee_split: to_web30_split(policy),
            treasury_account,
            node_reward_pool,
            service_provider_pool,
            unlock_controller,
        })
        .map_err(|e| from_web30_error("restore web30 mainnet token", e))?;

        Ok(Self {
            token,
            minted_locked_total: snapshot.minted_locked_total,
            burned_total: snapshot.burned_total,
            treasury_spent_total: snapshot.treasury_spent_total,
            tracked_accounts,
            treasury_account,
            node_reward_pool,
            service_provider_pool,
            unlock_controller,
        })
    }

    fn rebuild_with_policy(&mut self, policy: &TokenEconomicsPolicy) -> BFTResult<()> {
        let current_total = self.total_supply()?;
        if current_total > policy.max_supply {
            return Err(BFTError::InvalidProposal(format!(
                "token policy max_supply {} is below current total_supply {}",
                policy.max_supply, current_total
            )));
        }
        if self.minted_locked_total > policy.locked_supply {
            return Err(BFTError::InvalidProposal(format!(
                "token policy locked_supply {} is below already minted_locked {}",
                policy.locked_supply, self.minted_locked_total
            )));
        }

        let remaining_locked = policy
            .locked_supply
            .saturating_sub(self.minted_locked_total);
        let mut allocations: Vec<(Web30Address, u128)> = Vec::new();
        for address in &self.tracked_accounts {
            let balance = self.token.balance_of(address);
            if balance > 0 {
                allocations.push((*address, balance));
            }
        }

        self.token = MainnetTokenImpl::new(MainnetTokenConfig {
            name: self.token.name().to_string(),
            symbol: self.token.symbol().to_string(),
            decimals: self.token.decimals(),
            max_supply: policy.max_supply as u128,
            initial_allocations: allocations,
            locked_supply: remaining_locked as u128,
            fee_split: to_web30_split(policy),
            treasury_account: self.treasury_account,
            node_reward_pool: self.node_reward_pool,
            service_provider_pool: self.service_provider_pool,
            unlock_controller: self.unlock_controller,
        })
        .map_err(|e| from_web30_error("rebuild web30 mainnet token with new policy", e))?;
        Ok(())
    }

    pub fn reconfigure(&mut self, policy: &TokenEconomicsPolicy) -> BFTResult<()> {
        policy.validate()?;
        self.rebuild_with_policy(policy)
    }

    pub fn mint(&mut self, account: NodeId, amount: u64) -> BFTResult<()> {
        if amount == 0 {
            return Err(BFTError::InvalidProposal(
                "mint amount must be > 0".to_string(),
            ));
        }
        let address = node_to_address(account);
        self.tracked_accounts.insert(address);
        let event = self
            .token
            .mint(&address, amount as u128)
            .map_err(|e| from_web30_error("mint", e))?;
        match event {
            MainnetTokenEvent::Mint { amount: minted, .. } => {
                let minted = to_u64(minted, "mint amount")?;
                self.minted_locked_total = self
                    .minted_locked_total
                    .checked_add(minted)
                    .ok_or_else(|| {
                        BFTError::Internal("minted_locked_total overflow".to_string())
                    })?;
                Ok(())
            }
            _ => Err(BFTError::Internal(
                "unexpected non-mint event from mainnet token mint".to_string(),
            )),
        }
    }

    pub fn burn(&mut self, account: NodeId, amount: u64) -> BFTResult<()> {
        if amount == 0 {
            return Err(BFTError::InvalidProposal(
                "burn amount must be > 0".to_string(),
            ));
        }
        let address = node_to_address(account);
        self.tracked_accounts.insert(address);
        let event = self
            .token
            .burn(&address, amount as u128)
            .map_err(|e| from_web30_error("burn", e))?;
        match event {
            MainnetTokenEvent::Burn { amount: burned, .. } => {
                let burned = to_u64(burned, "burn amount")?;
                self.burned_total = self
                    .burned_total
                    .checked_add(burned)
                    .ok_or_else(|| BFTError::Internal("burned_total overflow".to_string()))?;
                Ok(())
            }
            _ => Err(BFTError::Internal(
                "unexpected non-burn event from mainnet token burn".to_string(),
            )),
        }
    }

    pub fn charge_gas_fee(&mut self, payer: NodeId, amount: u64) -> BFTResult<FeeRoutingOutcome> {
        if amount == 0 {
            return Err(BFTError::InvalidProposal(
                "fee amount must be > 0".to_string(),
            ));
        }
        let payer_address = node_to_address(payer);
        self.tracked_accounts.insert(payer_address);
        let event = self
            .token
            .on_gas_fee_paid(&payer_address, amount as u128)
            .map_err(|e| from_web30_error("charge gas fee", e))?;
        match event {
            MainnetTokenEvent::GasFeeRouted {
                to_node,
                to_treasury,
                to_burn,
                ..
            } => {
                self.burned_total = self
                    .burned_total
                    .checked_add(to_u64(to_burn, "gas fee burn")?)
                    .ok_or_else(|| BFTError::Internal("burned_total overflow".to_string()))?;
                Ok(FeeRoutingOutcome {
                    provider_amount: to_u64(to_node, "gas fee node share")?,
                    treasury_amount: to_u64(to_treasury, "gas fee treasury share")?,
                    burn_amount: to_u64(to_burn, "gas fee burn share")?,
                })
            }
            _ => Err(BFTError::Internal(
                "unexpected non-gas event from mainnet token gas fee".to_string(),
            )),
        }
    }

    pub fn charge_service_fee(
        &mut self,
        payer: NodeId,
        amount: u64,
    ) -> BFTResult<FeeRoutingOutcome> {
        if amount == 0 {
            return Err(BFTError::InvalidProposal(
                "fee amount must be > 0".to_string(),
            ));
        }
        let payer_address = node_to_address(payer);
        self.tracked_accounts.insert(payer_address);
        let event = self
            .token
            .on_service_fee_paid([0u8; 32], &payer_address, amount as u128)
            .map_err(|e| from_web30_error("charge service fee", e))?;
        match event {
            MainnetTokenEvent::ServiceFeeRouted {
                to_provider,
                to_treasury,
                to_burn,
                ..
            } => {
                self.burned_total = self
                    .burned_total
                    .checked_add(to_u64(to_burn, "service fee burn")?)
                    .ok_or_else(|| BFTError::Internal("burned_total overflow".to_string()))?;
                Ok(FeeRoutingOutcome {
                    provider_amount: to_u64(to_provider, "service fee provider share")?,
                    treasury_amount: to_u64(to_treasury, "service fee treasury share")?,
                    burn_amount: to_u64(to_burn, "service fee burn share")?,
                })
            }
            _ => Err(BFTError::Internal(
                "unexpected non-service event from mainnet token service fee".to_string(),
            )),
        }
    }

    pub fn spend_treasury(&mut self, to: NodeId, amount: u64) -> BFTResult<()> {
        if amount == 0 {
            return Err(BFTError::InvalidProposal(
                "treasury spend amount must be > 0".to_string(),
            ));
        }
        let to_address = node_to_address(to);
        self.tracked_accounts.insert(to_address);
        self.token
            .transfer(&self.treasury_account, &to_address, amount as u128)
            .map_err(|e| from_web30_error("treasury spend transfer", e))?;
        self.treasury_spent_total = self
            .treasury_spent_total
            .checked_add(amount)
            .ok_or_else(|| BFTError::Internal("treasury_spent_total overflow".to_string()))?;
        Ok(())
    }

    pub fn balance(&self, account: NodeId) -> BFTResult<u64> {
        to_u64(
            self.token.balance_of(&node_to_address(account)),
            "token account balance",
        )
    }

    pub fn total_supply(&self) -> BFTResult<u64> {
        to_u64(self.token.total_supply(), "token total_supply")
    }

    pub fn locked_minted_total(&self) -> u64 {
        self.minted_locked_total
    }

    pub fn treasury_balance(&self) -> BFTResult<u64> {
        to_u64(
            self.token.balance_of(&self.treasury_account),
            "treasury balance",
        )
    }

    pub fn burned_total(&self) -> u64 {
        self.burned_total
    }

    pub fn treasury_spent_total(&self) -> u64 {
        self.treasury_spent_total
    }

    pub fn gas_provider_pool_balance(&self) -> BFTResult<u64> {
        to_u64(
            self.token.balance_of(&self.node_reward_pool),
            "gas provider pool balance",
        )
    }

    pub fn service_provider_pool_balance(&self) -> BFTResult<u64> {
        to_u64(
            self.token.balance_of(&self.service_provider_pool),
            "service provider pool balance",
        )
    }

    /// 导出可用于分红快照的账户余额（按地址字典序稳定排序）。
    pub fn dividend_eligible_balances(&self, min_balance: u128) -> Vec<(Web30Address, u128)> {
        let mut balances: Vec<(Web30Address, u128)> = self
            .tracked_accounts
            .iter()
            .copied()
            .filter_map(|account| {
                let balance = self.token.balance_of(&account);
                if balance >= min_balance {
                    Some((account, balance))
                } else {
                    None
                }
            })
            .collect();
        balances.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
        balances
    }
}
