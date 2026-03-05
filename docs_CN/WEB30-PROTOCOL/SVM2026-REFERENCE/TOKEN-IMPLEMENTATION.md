# SuperVM 主链 Token 经济模型 - 技术实现说明

**更新日期**: 2025-01-17  
**实现状态**: ✅ 核心模块完成 (68/68 测试通过)

**相关文档**:
- 📊 [经济模块完成报告](../../ECONOMIC-MODULES-COMPLETION-REPORT.md) - 详细测试结果
- 🗺️ [开发路线图](../../ROADMAP.md) - Token & 经济模型章节
- 💼 [经济模型与治理](../../docs/商业计划/经济模型与治理.md) - 商业设计

---

## 📁 代码结构

```
contracts/web30/core/src/
├── mainnet_token.rs          # 主链 Token 核心接口
├── mainnet_token_impl.rs     # ✅ 实现完成 (416 行, 4 测试)
├── treasury.rs               # 国库管理(收入自动/支出治理)
├── treasury_impl.rs          # ✅ 实现完成 (375 行, 8 测试)
├── foreign_payment.rs        # 外币支付处理(归集+替代支付)
├── foreign_payment_impl.rs   # ✅ 实现完成 (445 行, 8 测试)
├── dividend_pool.rs          # 分红池(每日快照+主动领取)
├── governance.rs             # 治理模块(九人加权议会)
├── e2e_integration.rs        # ✅ 端到端测试 (269 行, 5 测试)
├── amm.rs                    # ✅ AMM 做市商 (451 行, 7 测试)
├── nav_redemption.rs         # ✅ NAV 刚性兑付 (336 行, 7 测试)
├── bonds.rs                  # ✅ 债券融资 (412 行, 8 测试)
├── cdp.rs                    # ✅ CDP 稳定币 (523 行, 9 测试)
├── economic_integration.rs   # ✅ 经济系统集成 (227 行, 2 测试)
└── lib.rs                    # 模块导出
```

**总计**: ~3,500 行代码, 68 个测试全部通过

## 🔧 核心模块说明

### 1. mainnet_token.rs - 主链 Token

**职责**:
- M0/M1/M2 货币层级管理
- Gas/手续费自动拆分路由
- 铸造/销毁控制
- 解锁时间表执行

**关键接口**:
```rust
pub trait MainnetToken {
    fn on_gas_fee_paid(&mut self, payer: &Address, amount: u128) -> Result<MainnetTokenEvent>;
    fn on_service_fee_paid(&mut self, service_id: [u8; 32], payer: &Address, amount: u128) -> Result<MainnetTokenEvent>;
    fn mint(&mut self, to: &Address, amount: u128) -> Result<MainnetTokenEvent>;
    fn burn(&mut self, from: &Address, amount: u128) -> Result<MainnetTokenEvent>;
}
```

**费用拆分示例**:
```rust
FeeSplit {
    gas_base_burn_bp: 1000,      // 10% Gas 销毁
    gas_to_node_bp: 6000,        // 60% 给矿工
    service_burn_bp: 1000,       // 10% 手续费销毁
    service_to_provider_bp: 5000, // 50% 给服务提供者
}
// 剩余部分自动路由到分红池和国库
```

### 2. treasury.rs - 国库

**职责**:
- 接收协议收入(自动)
- 支付矿工激励(用 Token 替代外币)
- 执行治理支出决策
- 回购销毁策略

**关键事件**:
```rust
pub enum TreasuryEvent {
    Income { from: Address, amount: u128, account: TreasuryAccountKind },
    Spend { to: Address, amount: u128, reason: String },
    ForeignCurrencyCollected { currency: String, amount: u128 },
    MinerPaidInToken { miner: Address, token_amount: u128, equivalent_foreign: u128 },
}
```

**账户分类**:
- `Main`: 主国库(一般运营)
- `Ecosystem`: 生态基金(DApp 补贴/黑客松)
- `RiskReserve`: 风险准备金(极端事件兜底)

### 3. foreign_payment.rs - 外币支付

**职责**:
- 接收外部链支付(BTC/ETH/USDT)
- 归集到外汇储备池
- 计算 AMM 汇率
- 用 Token 支付给矿工
- 矿工兑换外币

**核心流程**:
```rust
impl ForeignPaymentProcessor {
    fn process_foreign_payment(
        &mut self,
        payment: ForeignPayment,  // 外币支付信息
        miner: Address,
    ) -> Result<MinerPayment> {
        // 1. 归集外币到储备池
        self.collect_to_reserve(payment)?;
        
        // 2. 查询 AMM 汇率
        let (token_amount, rate) = self.calculate_token_equivalent(
            &payment.currency,
            payment.amount
        )?;
        
        // 3. 从国库支付 Token
        self.pay_miner_in_token(miner, token_amount, payment)
    }
}
```

**储备健康指标**:
```rust
impl ForeignPaymentStats {
    fn calculate_reserve_ratio(&self, currency: &str) -> f64;  // 储备率
    fn calculate_hold_preference(&self, currency: &str) -> f64; // 持有倾向
}
```

### 4. dividend_pool.rs - 分红池

**职责**:
- 每日 UTC 00:00 快照持仓
- 计算每日可领取分红
- 用户主动领取(防 Gas 浪费)
- 累计未领取分红

**快照机制**:
```rust
pub struct DailySnapshot {
    pub day: u64,                  // Unix 天数
    pub pool_income: u128,         // 当日分红池收入
    pub total_circulating: u128,   // 总流通量
    pub balances: HashMap<Address, u128>, // 用户持仓快照
}
```

**领取流程**:
```rust
impl DividendPool {
    fn get_claimable(&self, user: &Address) -> Result<(u128, u64)> {
        // 计算: Σ (user_balance[day] / total_supply[day] × pool_income[day])
        // 返回: (可领取金额, 累计天数)
    }
    
    fn claim(&mut self, user: &Address) -> Result<DividendEvent> {
        // 限制: 每日最多领取一次, 最小持仓 100 Token
    }
}
```

### 5. governance.rs - 治理

**职责**:
- 九人加权议会管理
- 提案创建与投票
- 投票权委托(小户 → 大户)
- 提案执行

**议会席位**:
```rust
pub enum CouncilSeat {
    Founder,           // 35% 创始人
    TopHolder(u8),     // 10% × 5 = 50% Top5 持币者
    Team(u8),          // 5% × 2 = 10% 团队
    Independent,       // 5% 独立委员
}
```

**提案类型与阈值**:
```rust
pub enum ProposalType {
    ParameterChange,   // 参数调整: >50% 通过
    TreasurySpend,     // 国库支出: >66% 通过
    ProtocolUpgrade,   // 协议升级: >75% 通过
    EmergencyFreeze,   // 紧急冻结: >50% + 3个不同类别席位
}
```

**投票委托**:
```rust
impl Governance {
    fn delegate_vote(&mut self, delegator: &Address, delegatee: &Address, amount: u128) -> Result<GovernanceEvent>;
    fn get_delegated_power(&self, delegatee: &Address) -> u128; // 查询被委托的总投票权
}
```

## 🔄 完整交互流程

### 场景 1: 本币 Gas 费用

```
用户发起交易
    ↓
VM 执行交易,消耗 100 Token Gas
    ↓
MainnetToken::on_gas_fee_paid(user, 100)
    ↓
费用拆分:
├─ 60 Token → 矿工节点(直接激励)
├─ 30 Token → 分红池(DividendPool::receive_income)
└─ 10 Token → BaseFee 销毁(MainnetToken::burn)
    ↓
次日 00:00 DividendPool::take_daily_snapshot()
    ↓
用户调用 DividendPool::claim()
    ↓
获得分红 Token
```

### 场景 2: 外币跨链服务

```
比特币用户支付 0.001 BTC 使用 SuperVM
    ↓
ForeignPaymentProcessor::process_foreign_payment(BTC, 0.001, miner)
    ↓
1. 归集: 0.001 BTC → BTC 储备池(增强 AMM 流动性)
2. 查询汇率: Token/BTC Pool (x·y=k)
3. 计算等值: 0.001 BTC × 5000 = 5 Token
4. 国库支付: Treasury::pay_miner_in_token(miner, 5)
    ↓
矿工收到 5 Token,选择:
├─ 持有 → 享受每日分红
└─ 兑换 → ForeignPaymentProcessor::miner_swap_to_foreign(5, "BTC")
            └─ 从 BTC 储备池兑换 BTC(汇率浮动)
```

### 场景 3: 治理提案

```
议会成员创建提案: "调整 Gas BaseFee 销毁比例 10% → 15%"
    ↓
Governance::create_proposal(proposer, ParameterChange, ...)
    ↓
9 人投票(7天投票期):
- 创始人(35%): ✅ 赞成
- Top5(50%): ✅✅✅ 赞成(3人), ❌ 反对(1人), ⚪ 弃权(1人)
- 团队(10%): ✅✅ 赞成(2人)
- 独立(5%): ❌ 反对
    ↓
Governance::finalize_proposal()
统计: 赞成 75%, 反对 15%, 弃权 10%
阈值: >50% → ✅ 通过
    ↓
Governance::execute_proposal()
调用 MainnetToken::set_fee_split(FeeSplit { gas_base_burn_bp: 1500, ... })
    ↓
新参数生效,后续交易 15% Gas 销毁
```

## 📊 数据流向图

```
┌──────────────────────────────────────────────────────────┐
│                      用户支付层                            │
│  ┌──────────────┐              ┌──────────────┐          │
│  │ 本币(Token)  │              │ 外币(BTC等)  │          │
│  └──────┬───────┘              └──────┬───────┘          │
└─────────┼──────────────────────────────┼──────────────────┘
          │                              │
          ▼                              ▼
┌──────────────────┐          ┌───────────────────────┐
│ MainnetToken     │          │ ForeignPaymentProcessor│
│ - on_gas_fee     │          │ - collect_to_reserve   │
│ - on_service_fee │          │ - calculate_token      │
└─────┬────────────┘          └──────┬────────────────┘
      │ 拆分                         │ 等值计算
      ├─────┬─────┬─────┐           │
      ▼     ▼     ▼     ▼           ▼
   矿工  分红池 国库  销毁    ┌──────────────┐
    60%   30%  0%   10%      │ Treasury     │
      │     │     │     │     │ - pay_miner  │
      │     │     │     │     └──────┬───────┘
      │     │     │     │            │ Token替代
      │     ▼     │     ▼            ▼
      │  ┌────────────┐ 🔥        矿工地址
      │  │DividendPool│ Burn         │
      │  │- snapshot  │              │
      │  │- claim     │              ├─ 持有 → 分红池
      │  └─────┬──────┘              └─ 兑换 → AMM池
      │        │                          ▼
      │        ▼                   ┌──────────────┐
      │   用户主动领取              │ ForeignReserve│
      │        │                   │ - AMM Pool    │
      │        ▼                   │ - swap        │
      └────→ 💰                    └───────────────┘
           持币分红                    外汇储备
                                    (多资产背书)
```

## 🚀 部署流程

1. **初始化 MainnetToken**
   ```rust
   let token = MainnetTokenImpl::new(
       "SuperVM Token".to_string(),
       "SVM".to_string(),
       18,
       100_000_000 * 10^18, // 1亿 Max Supply
   );
   ```

2. **初始化国库**
   ```rust
   let treasury = TreasuryImpl::new();
   treasury.allocate(TreasuryAccountKind::Main, 10_000_000 * 10^18);
   ```

3. **初始化分红池**
   ```rust
   let dividend_pool = DividendPoolImpl::new(100 * 10^18); // 最小 100 Token
   ```

4. **初始化治理**
   ```rust
   let governance = GovernanceImpl::new();
   governance.set_council_member(CouncilSeat::Founder, founder_addr)?;
   // ... 设置其他成员
   ```

5. **初始化外币支付**
   ```rust
   let foreign_payment = ForeignPaymentProcessorImpl::new();
   // 注入初始流动性到 AMM 池
   ```

6. **连接模块**
   ```rust
   token.set_dividend_pool(dividend_pool);
   token.set_treasury(treasury);
   token.set_governance(governance);
   ```

---

**参考文档**:
- [经济模型与治理](../../docs/商业计划/经济模型与治理.md)
- [ROADMAP.md](../../ROADMAP.md) - Token 经济路线图
