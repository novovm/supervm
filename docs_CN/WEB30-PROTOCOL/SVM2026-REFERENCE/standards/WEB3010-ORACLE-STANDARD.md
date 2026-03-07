# WEB3010: 预言机标准

**版本**: v0.1.0  
**状态**: Draft  
**作者**: SuperVM Core Team  

---

## 设计理念

WEB3010 是 SuperVM 原生预言机协议，利用 zkVM 验证数据真实性、AI 聚合多源数据。

## 核心创新

| Chainlink | **WEB3010** |
|-----------|-------------|
| 节点投票 | ✅ **zkVM 可验证数据源** |
| 单源数据 | ✅ **AI 多源聚合** |
| 延迟高 | ✅ **实时流式推送** |

---

## Rust Trait 接口

```rust
#[async_trait::async_trait]
pub trait WEB3010Oracle {
    // ============ 数据上报 ============
    
    /// 上报价格数据
    async fn report_price(
        &self,
        asset: String,  // e.g., "BTC/USD"
        price: f64,
        source: DataSource,
        proof: ZkProof,  // 证明数据来自可信源
    ) -> Result<TransactionHash, OracleError>;
    
    /// 上报自定义数据
    async fn report_data(
        &self,
        feed_id: FeedId,
        data: Vec<u8>,
        proof: ZkProof,
    ) -> Result<TransactionHash, OracleError>;
    
    // ============ 数据查询 ============
    
    /// 查询最新价格
    async fn get_latest_price(
        &self,
        asset: String,
    ) -> Result<PriceData, OracleError>;
    
    /// 查询历史价格
    async fn get_historical_price(
        &self,
        asset: String,
        timestamp: u64,
    ) -> Result<PriceData, OracleError>;
    
    // ============ AI 聚合 ============
    
    /// AI 聚合多源数据（去除异常值）
    async fn ai_aggregate_prices(
        &self,
        asset: String,
        sources: Vec<DataSource>,
    ) -> Result<f64, OracleError>;
    
    /// AI 预测未来价格
    async fn ai_predict_price(
        &self,
        asset: String,
        time_horizon: u64,
    ) -> Result<PricePrediction, OracleError>;
    
    // ============ 订阅推送 ============
    
    /// 订阅价格更新
    async fn subscribe_price_updates(
        &self,
        asset: String,
        callback: Box<dyn Fn(PriceData) + Send>,
    ) -> Result<SubscriptionId, OracleError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSource {
    Binance,
    Coinbase,
    Kraken,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    pub asset: String,
    pub price: f64,
    pub timestamp: u64,
    pub sources: Vec<DataSource>,
}

pub type FeedId = [u8; 32];
```

---

## 应用场景

### **DeFi 喂价**
```rust
// 借贷协议查询 BTC 价格（AI 聚合多源，去除异常值）
let btc_price = oracle.ai_aggregate_prices(
    "BTC/USD".to_string(),
    vec![DataSource::Binance, DataSource::Coinbase, DataSource::Kraken],
).await?;
```

---

## Roadmap

| 阶段 | 功能 | 状态 |
|------|------|------|
| **Phase 1** | 价格预言机 | 📋 设计中 |
| **Phase 2** | zkVM 数据验证 | 📋 规划中 |
| **Phase 3** | AI 多源聚合 | 📋 规划中 |
| **Phase 4** | 实时流式推送 | 📋 规划中 |
