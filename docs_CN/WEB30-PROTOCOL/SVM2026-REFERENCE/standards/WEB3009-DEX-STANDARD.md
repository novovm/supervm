# WEB3009: 去中心化交易所标准 (DEX)

**版本**: v0.1.0  
**状态**: Draft  
**作者**: SuperVM Core Team  

---

## 设计理念

WEB3009 是 SuperVM 原生 DEX 协议，利用 MVCC 并行、AI 做市、顺序套利保护。

## 核心创新

| Uniswap | **WEB3009** |
|---------|-------------|
| 串行交易 | ✅ **MVCC 并行 495K TPS** |
| 固定费率 | ✅ **AI 动态费率** |
| 顺序套利抢跑严重 | ✅ **隐私订单（环签名）** |
| 单链流动性 | ✅ **跨链聚合** |

---

## Rust Trait 接口

```rust
#[async_trait::async_trait]
pub trait WEB3009DEX {
    // ============ 交易 ============
    
    /// 限价单
    async fn place_limit_order(
        &self,
        token_in: Address,
        token_out: Address,
        amount_in: u128,
        price: f64,
    ) -> Result<OrderId, DEXError>;
    
    /// 市价单
    async fn place_market_order(
        &self,
        token_in: Address,
        token_out: Address,
        amount_in: u128,
        min_amount_out: u128,
    ) -> Result<TransactionHash, DEXError>;
    
    /// 隐私订单（防顺序套利抢跑）
    async fn place_private_order(
        &self,
        order: Order,
        ring_signature: RingSignature,
    ) -> Result<OrderId, DEXError>;
    
    // ============ AI 做市 ============
    
    /// AI 优化流动性分配
    async fn ai_optimize_liquidity(
        &self,
        pool: Address,
    ) -> Result<LiquidityStrategy, DEXError>;
    
    /// AI 预测价格
    async fn ai_predict_price(
        &self,
        token: Address,
        time_horizon: u64,  // 秒
    ) -> Result<PricePrediction, DEXError>;
    
    // ============ 跨链交易 ============
    
    /// 跨链 Swap（SuperVM USDT → Ethereum ETH）
    async fn cross_chain_swap(
        &self,
        token_in: (ChainId, Address),
        token_out: (ChainId, Address),
        amount_in: u128,
    ) -> Result<CrossChainReceipt, DEXError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: u128,
    pub price: Option<f64>,  // None = 市价单
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricePrediction {
    pub predicted_price: f64,
    pub confidence: f32,
    pub trend: Trend,
}

pub type OrderId = [u8; 32];
```

---

## 应用场景

### **AI 做市商**
```rust
// AI 根据市场波动自动调整流动性
let strategy = dex.ai_optimize_liquidity(usdt_eth_pool).await?;
println!("AI 建议: {:?}", strategy);
```

---

## Roadmap

| 阶段 | 功能 | 状态 |
|------|------|------|
| **Phase 1** | AMM + 订单簿 | 📋 设计中 |
| **Phase 2** | 隐私订单 | 📋 规划中 |
| **Phase 3** | AI 做市 | 📋 规划中 |
| **Phase 4** | 跨链交易 | 📋 规划中 |

---

## 与 AtomicCrossChainSwap 的关系（落地对齐）

- 结算层：WEB3009 的跨链结算应直接复用 L0 已实现的原子交换执行器。
    - 源码位置：`src/vm-runtime/src/adapter/atomic_swap.rs`
    - 核心类型：`AtomicCrossChainSwap`、`SwapRequest`、`AssetAmount`、`AssetType`、`SwapReceipt`
    - 关键属性：单事务（MVCC）原子性、跨链账户关联、失败即回滚、收据持久化

- 协议映射：
    - `WEB3009DEX::cross_chain_swap(...)` → 构造 `SwapRequest` 并委托 `AtomicCrossChainSwap::execute_atomic_swap()`；返回值对齐为 `SwapReceipt` 的轻量封装 `CrossChainReceipt`。
    - 订单撮合（限价/市价/隐私订单）位于撮合层或 AMM 层；撮合成功后调用上述结算层原语完成资产原子交换。

- 当前状态与TODO：
    - ✅ 原子交换执行器已实现（签名校验与 nonce/索引查询有 TODO）。
    - ⏳ 订单簿/AMM/路径路由、做市策略与风控仍属上层实现（本标准定义接口）。
    - 🔒 隐私订单建议结合 `RingCT` 完成签名与双花检测，再落到同一结算原语。

> 结论：WEB3009 与“原子交易匹配”的关系是“上层撮合/定价 + 下层原子结算”。目前下层原子结算（AtomicCrossChainSwap）已具备，可直接作为 DEX 的跨链结算基石。

