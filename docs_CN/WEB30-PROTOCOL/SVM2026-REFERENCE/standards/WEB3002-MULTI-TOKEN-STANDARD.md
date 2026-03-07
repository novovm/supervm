# WEB3002: 多代币标准 (Multi-Token)

**版本**: v0.1.0  
**状态**: Draft  
**作者**: SuperVM Core Team  

---

## 设计理念

WEB3002 是 SuperVM 的 ERC1155 增强版，支持同时管理 FT（可替代代币）和 NFT（非同质化代币），但增加了 MVCC 并行、跨链和隐私能力。

## 对比：ERC1155 vs WEB3002

| 特性 | ERC1155 | **WEB3002** |
|------|---------|-------------|
| **FT+NFT 混合** | ✅ 支持 | ✅ **支持** |
| **批量操作** | ✅ Gas 优化 | ✅ **MVCC 并行** |
| **跨链转移** | ❌ 需桥接 | ✅ **原生跨链** |
| **隐私转移** | ❌ 无 | ✅ **环签名** |
| **原子交换** | ⚠️ 需外部合约 | ✅ **内置原子交换** |
| **动态供应** | ⚠️ 需权限 | ✅ **DAO 治理铸造** |

---

## Rust Trait 接口

```rust
#[async_trait::async_trait]
pub trait WEB3002MultiToken {
    // ============ 铸造 ============
    
    /// 铸造代币（FT 或 NFT）
    async fn mint(
        &self,
        to: Address,
        token_id: TokenId,
        amount: u128,  // FT: >1, NFT: =1
        data: Vec<u8>,
    ) -> Result<TransactionHash, MultiTokenError>;
    
    /// 批量铸造
    async fn batch_mint(
        &self,
        to: Address,
        token_ids: Vec<TokenId>,
        amounts: Vec<u128>,
    ) -> Result<TransactionHash, MultiTokenError>;
    
    // ============ 转移 ============
    
    /// 单个转移
    async fn safe_transfer(
        &self,
        from: Address,
        to: Address,
        token_id: TokenId,
        amount: u128,
    ) -> Result<TransferReceipt, MultiTokenError>;
    
    /// 批量转移（多种代币一次性转移）
    async fn safe_batch_transfer(
        &self,
        from: Address,
        to: Address,
        token_ids: Vec<TokenId>,
        amounts: Vec<u128>,
    ) -> Result<Vec<TransferReceipt>, MultiTokenError>;
    
    /// 跨链批量转移
    async fn cross_chain_batch_transfer(
        &self,
        token_ids: Vec<TokenId>,
        amounts: Vec<u128>,
        target_chain: ChainId,
        recipient: Address,
    ) -> Result<CrossChainReceipt, MultiTokenError>;
    
    // ============ 原子交换 ============
    
    /// 原子交换（Alice 的 Token A ↔ Bob 的 Token B）
    async fn atomic_swap(
        &self,
        party_a: SwapParty,
        party_b: SwapParty,
    ) -> Result<SwapReceipt, MultiTokenError>;
    
    // ============ 查询 ============
    
    fn balance_of(&self, owner: Address, token_id: TokenId) -> Result<u128, MultiTokenError>;
    
    fn balance_of_batch(
        &self,
        owners: Vec<Address>,
        token_ids: Vec<TokenId>,
    ) -> Result<Vec<u128>, MultiTokenError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapParty {
    pub address: Address,
    pub token_ids: Vec<TokenId>,
    pub amounts: Vec<u128>,
}
```

---

## 应用场景

### **游戏道具系统**
```rust
// 一次性转移：100 金币 + 1 史诗武器 + 5 药水
multi_token.safe_batch_transfer(
    player_a,
    player_b,
    vec![gold_id, sword_id, potion_id],
    vec![100, 1, 5],
).await?;
```

---

## Roadmap

| 阶段 | 功能 | 状态 |
|------|------|------|
| **Phase 1** | 基础 FT+NFT 混合 | 📋 设计中 |
| **Phase 2** | 原子交换 | 📋 规划中 |
| **Phase 3** | 跨链批量转移 | 📋 规划中 |
