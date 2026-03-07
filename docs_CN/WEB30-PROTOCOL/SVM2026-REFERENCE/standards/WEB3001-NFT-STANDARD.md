# WEB3001: 非同质化代币标准 (NFT)

**版本**: v0.1.0  
**状态**: Draft  
**作者**: SuperVM Core Team  

---

## 设计理念

WEB3001 是 SuperVM 原生 NFT 标准，充分利用 L0 MVCC、跨链、隐私和 zkVM 能力，超越 ERC721/ERC1155。

## 对比：主流 NFT 标准

| 特性 | ERC721 | ERC1155 | Solana Metaplex | **WEB3001** |
|------|--------|---------|-----------------|-------------|
| **并行铸造** | ❌ 串行 | ❌ 串行 | ⚠️ 有限并行 | ✅ **MVCC 无限并行** |
| **跨链原生** | ❌ 需桥接 | ❌ 需桥接 | ❌ 需桥接 | ✅ **原生跨链转移** |
| **隐私转移** | ❌ 公开 | ❌ 公开 | ❌ 公开 | ✅ **环签名隐身地址** |
| **动态属性** | ⚠️ 需升级合约 | ⚠️ 需升级合约 | ⚠️ 链下元数据 | ✅ **链上可变状态** |
| **批量操作** | ❌ Gas 高 | ✅ 支持 | ✅ 支持 | ✅ **原子批量** |
| **版税执行** | ⚠️ 可绕过 | ⚠️ 可绕过 | ✅ 强制执行 | ✅ **链上强制 + zkVM** |
| **可组合性** | ⚠️ 有限 | ⚠️ 有限 | ⚠️ 有限 | ✅ **NFT 可嵌套/拆分** |
| **AI 集成** | ❌ 无 | ❌ 无 | ❌ 无 | ✅ **WEB3011 原生支持** |

---

## Rust Trait 接口

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// WEB3001 NFT 核心 Trait
#[async_trait::async_trait]
pub trait WEB3001NFT {
    // ============ 铸造 ============
    
    /// 单个铸造
    async fn mint(
        &self,
        to: Address,
        token_id: TokenId,
        metadata_uri: String,
        royalty: Option<Royalty>,
    ) -> Result<TransactionHash, NFTError>;
    
    /// 批量铸造（并行）
    async fn batch_mint(
        &self,
        recipients: Vec<Address>,
        token_ids: Vec<TokenId>,
        metadata_uris: Vec<String>,
    ) -> Result<Vec<TransactionHash>, NFTError>;
    
    /// 懒铸造（先预留，用户购买时才真正铸造）
    async fn lazy_mint(
        &self,
        voucher: MintVoucher,
        signature: Signature,
    ) -> Result<TokenId, NFTError>;
    
    // ============ 转移 ============
    
    /// 标准转移
    async fn transfer(
        &self,
        from: Address,
        to: Address,
        token_id: TokenId,
    ) -> Result<TransferReceipt, NFTError>;
    
    /// 隐私转移（环签名 + 隐身地址）
    async fn transfer_private(
        &self,
        token_id: TokenId,
        stealth_address: StealthAddress,
        ring_signature: RingSignature,
    ) -> Result<TransferReceipt, NFTError>;
    
    /// 跨链转移
    async fn transfer_cross_chain(
        &self,
        token_id: TokenId,
        target_chain: ChainId,
        recipient: Address,
    ) -> Result<CrossChainReceipt, NFTError>;
    
    /// 批量转移
    async fn batch_transfer(
        &self,
        from: Address,
        to: Address,
        token_ids: Vec<TokenId>,
    ) -> Result<Vec<TransferReceipt>, NFTError>;
    
    // ============ 动态属性 ============
    
    /// 更新链上属性（动态 NFT）
    async fn update_attribute(
        &self,
        token_id: TokenId,
        key: String,
        value: AttributeValue,
    ) -> Result<TransactionHash, NFTError>;
    
    /// 升级/进化 NFT
    async fn evolve(
        &self,
        token_id: TokenId,
        evolution_data: EvolutionData,
    ) -> Result<TransactionHash, NFTError>;
    
    // ============ 可组合性 ============
    
    /// 嵌套 NFT（NFT 拥有 NFT）
    async fn attach_child(
        &self,
        parent_id: TokenId,
        child_id: TokenId,
    ) -> Result<TransactionHash, NFTError>;
    
    /// 分离子 NFT
    async fn detach_child(
        &self,
        parent_id: TokenId,
        child_id: TokenId,
    ) -> Result<TransactionHash, NFTError>;
    
    /// 分割 NFT（1个 NFT → N 个碎片）
    async fn fractionalize(
        &self,
        token_id: TokenId,
        fractions: u64,
    ) -> Result<Vec<TokenId>, NFTError>;
    
    /// 合并碎片
    async fn merge_fractions(
        &self,
        fraction_ids: Vec<TokenId>,
    ) -> Result<TokenId, NFTError>;
    
    // ============ 版税 ============
    
    /// 设置版税（创作者/平台分成）
    async fn set_royalty(
        &self,
        token_id: TokenId,
        recipients: Vec<Address>,
        percentages: Vec<u16>,  // 基点，10000 = 100%
    ) -> Result<TransactionHash, NFTError>;
    
    /// 自动分配版税收益
    async fn distribute_royalty(
        &self,
        token_id: TokenId,
        sale_price: u128,
    ) -> Result<Vec<TransactionHash>, NFTError>;
    
    // ============ 查询 ============
    
    /// 查询所有者
    fn owner_of(&self, token_id: TokenId) -> Result<Address, NFTError>;
    
    /// 查询余额
    fn balance_of(&self, owner: Address) -> Result<u64, NFTError>;
    
    /// 查询元数据
    async fn token_uri(&self, token_id: TokenId) -> Result<String, NFTError>;
    
    /// 查询链上属性
    async fn get_attributes(
        &self,
        token_id: TokenId,
    ) -> Result<HashMap<String, AttributeValue>, NFTError>;
    
    /// 查询子 NFT
    async fn get_children(&self, parent_id: TokenId) -> Result<Vec<TokenId>, NFTError>;
    
    // ============ 销毁 ============
    
    /// 销毁 NFT
    async fn burn(&self, token_id: TokenId) -> Result<TransactionHash, NFTError>;
    
    // ============ AI 集成 ============
    
    /// AI 生成元数据（调用 WEB3011）
    async fn ai_generate_metadata(
        &self,
        prompt: String,
        model: AIModel,
    ) -> Result<String, NFTError>;
    
    /// AI 驱动的动态属性
    async fn ai_update_attributes(
        &self,
        token_id: TokenId,
        context: String,
    ) -> Result<TransactionHash, NFTError>;
}

// ============ 数据结构 ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Royalty {
    pub recipients: Vec<Address>,
    pub percentages: Vec<u16>,  // 基点
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintVoucher {
    pub token_id: TokenId,
    pub metadata_uri: String,
    pub price: u128,
    pub currency: Address,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttributeValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<AttributeValue>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionData {
    pub new_metadata_uri: String,
    pub attribute_changes: HashMap<String, AttributeValue>,
}

pub type TokenId = [u8; 32];
```

---

## 应用场景

### 1. **游戏装备 NFT（动态属性）**
```rust
// 装备随使用升级
nft.update_attribute(
    sword_id,
    "attack_power".to_string(),
    AttributeValue::Number(150.0),  // 从 100 升到 150
).await?;
```

### 2. **音乐 NFT（强制版税）**
```rust
// 自动分配版税：70% 艺术家，20% 制作人，10% 平台
nft.set_royalty(
    music_nft_id,
    vec![artist, producer, platform],
    vec![7000, 2000, 1000],
).await?;
```

### 3. **AI 生成艺术 NFT**
```rust
// AI 生成元数据
let metadata = nft.ai_generate_metadata(
    "赛博朋克风格的龙".to_string(),
    AIModel::DALLE3,
).await?;
```

---

## Roadmap

| 阶段 | 功能 | 状态 |
|------|------|------|
| **Phase 1** | 基础 NFT 铸造/转移 | 📋 设计中 |
| **Phase 2** | 动态属性 | 📋 规划中 |
| **Phase 3** | 可组合性（嵌套/分割） | 📋 规划中 |
| **Phase 4** | 跨链 NFT | 📋 规划中 |
| **Phase 5** | AI 集成 | 📋 规划中 |
