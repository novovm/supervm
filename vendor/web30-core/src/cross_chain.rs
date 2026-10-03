//! 跨链转账功能

use crate::types::*;
use anyhow::{bail, Result};

/// 跨链协调器（简化版）
pub struct CrossChainCoordinator {
    pending_swaps: Vec<PendingSwap>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct PendingSwap {
    swap_id: [u8; 32],
    from_chain: ChainId,
    to_chain: ChainId,
    from_address: Address,
    to_address: Address,
    amount: u128,
    status: CrossChainStatus,
    created_at: u64,
}

impl CrossChainCoordinator {
    pub fn new() -> Self {
        Self {
            pending_swaps: Vec::new(),
        }
    }

    /// 发起跨链转账
    pub fn initiate_swap(
        &mut self,
        from_chain: ChainId,
        to_chain: ChainId,
        from_address: Address,
        to_address: Address,
        amount: u128,
    ) -> Result<[u8; 32]> {
        // 生成唯一的 swap ID
        let swap_id =
            self.generate_swap_id(from_chain, to_chain, &from_address, &to_address, amount);

        let swap = PendingSwap {
            swap_id,
            from_chain,
            to_chain,
            from_address,
            to_address,
            amount,
            status: CrossChainStatus::Pending,
            created_at: self.get_current_timestamp(),
        };

        self.pending_swaps.push(swap);

        Ok(swap_id)
    }

    /// 确认跨链转账
    pub fn confirm_swap(&mut self, swap_id: &[u8; 32]) -> Result<()> {
        let swap = self
            .pending_swaps
            .iter_mut()
            .find(|s| &s.swap_id == swap_id)
            .ok_or_else(|| anyhow::anyhow!("Swap not found"))?;

        if swap.status != CrossChainStatus::Pending {
            bail!("Swap is not in pending status");
        }

        swap.status = CrossChainStatus::Confirmed;
        Ok(())
    }

    /// 查询跨链转账状态
    pub fn get_swap_status(&self, swap_id: &[u8; 32]) -> Option<CrossChainStatus> {
        self.pending_swaps
            .iter()
            .find(|s| &s.swap_id == swap_id)
            .map(|s| s.status)
    }

    fn generate_swap_id(
        &self,
        from_chain: ChainId,
        to_chain: ChainId,
        from_address: &Address,
        to_address: &Address,
        amount: u128,
    ) -> [u8; 32] {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(from_chain.to_le_bytes());
        hasher.update(to_chain.to_le_bytes());
        hasher.update(from_address.as_bytes());
        hasher.update(to_address.as_bytes());
        hasher.update(amount.to_le_bytes());
        hasher.update(self.get_current_timestamp().to_le_bytes());

        hasher.finalize().into()
    }

    fn get_current_timestamp(&self) -> u64 {
        // 在实际实现中，这会从运行时获取
        0
    }
}

impl Default for CrossChainCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_chain_swap() {
        let mut coordinator = CrossChainCoordinator::new();

        let from_address = Address::from_bytes([1u8; 32]);
        let to_address = Address::from_bytes([2u8; 32]);

        let swap_id = coordinator
            .initiate_swap(1, 137, from_address, to_address, 1000)
            .expect("Failed to initiate swap");

        assert_eq!(
            coordinator.get_swap_status(&swap_id),
            Some(CrossChainStatus::Pending)
        );

        coordinator
            .confirm_swap(&swap_id)
            .expect("Failed to confirm");

        assert_eq!(
            coordinator.get_swap_status(&swap_id),
            Some(CrossChainStatus::Confirmed)
        );
    }
}
