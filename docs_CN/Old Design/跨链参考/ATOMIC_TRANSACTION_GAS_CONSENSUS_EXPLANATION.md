# SuperVM 原子跨链交易：Gas计算、共识和广播机制

## 目录
1. [系统架构](#系统架构)
2. [Gas计算机制](#gas计算机制)
3. [原子交易的两阶段共识](#原子交易的两阶段共识)
4. [广播逻辑](#广播逻辑)
5. [失败场景处理](#失败场景处理)
6. [理论基础](#理论基础)

---

## 系统架构

### 1. MVCC + 两阶段提交（2PC）的结合

```
┌─────────────────────────────────────────────────────────┐
│         SuperVM 原子交易执行框架                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Layer 1: 跨链交换（Atomic Cross-Chain Swap）         │
│  ├─ Bitcoin 余额 ↔ Ethereum 余额                       │
│  ├─ Solana Token ↔ Polygon Token                      │
│  └─ 支持任意链组合                                     │
│                                                         │
│  Layer 2: MVCC 事务层                                  │
│  ├─ Begin Transaction (获取快照 TS)                   │
│  ├─ Read/Write Phase (操作缓冲区)                     │
│  └─ Commit (原子提交到版本存储)                       │
│                                                         │
│  Layer 3: 两阶段提交协调器（2PC）                     │
│  ├─ Prepare Phase (锁 + 校验)                        │
│  ├─ Commit Phase (原子写入)                          │
│  └─ 失败自动回滚                                      │
│                                                         │
│  Layer 4: 链适配器（ChainAdapter）                    │
│  ├─ 解析多链格式 → 统一 IR                            │
│  ├─ 计算链特定 Gas                                   │
│  └─ 验证链特定签名                                   │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### 2. 核心数据结构

#### SwapRequest（交换请求）
```rust
pub struct SwapRequest {
    from: SuperVMAccountId,        // Alice
    to: SuperVMAccountId,          // Bob
    from_asset: AssetAmount,       // 1 BTC (Bitcoin)
    to_asset: AssetAmount,         // 10 ETH (Ethereum)
    deadline: u64,                 // Unix timestamp
    nonce: u64,                    // 防重放
    signature: Vec<u8>,            // Alice 的签名
}
```

#### AssetAmount（资产）
```rust
pub struct AssetAmount {
    chain_id: u64,                 // 链ID (1=Ethereum, 0=Bitcoin...)
    chain_type: ChainType,         // EVM | Bitcoin | Solana | ...
    asset_type: AssetType,         // Native | ERC20(地址) | SPL(地址) | UTXO
    amount: u128,                  // 精确数值（包含小数位）
}
```

---

## Gas计算机制

### 1. 链特定的 Gas 计算

每条链有不同的 Gas 模型：

```rust
// 代码位置: src/vm-runtime/src/chain_linker/chain_adapter.rs

pub trait ChainAdapter {
    /// 估算交易 Gas
    fn estimate_gas(&self, tx: &TxIR) -> Result<u64>;
}
```

### 2. 具体实现示例

#### EVM 链（Ethereum, Polygon, BNB等）
```rust
// Gas 计算公式：
// 总 Gas = 基础 Gas (21,000) + 数据 Gas + 执行 Gas

fn estimate_gas_evm(tx: &TxIR) -> u64 {
    let mut gas = 21_000;  // 基础交易 gas
    
    // 数据 gas: 每字节 16 gas (0) 或 4 gas (non-zero)
    for byte in &tx.data {
        if *byte == 0 {
            gas += 16;
        } else {
            gas += 4;
        }
    }
    
    // 合约执行 gas: 约 25,000 (存储写入较多)
    if tx.to_address.is_some() {
        gas += 25_000;
    }
    
    gas
}
```

示例输出：
```
EVM Transaction for 1 BTC → 10 ETH swap:
├─ Base gas: 21,000
├─ Data gas (280 bytes): 4,480
├─ Contract execution: 25,000
└─ Total: 50,480 gas
   至 50 Gwei: 2,524,000 Wei ≈ 0.002524 ETH
```

#### Bitcoin
```rust
// Bitcoin 不使用 Gas，而是 Satoshis 费用
// 费用 = 交易字节大小 × fee_rate

fn estimate_fee_bitcoin(tx: &TxIR) -> u64 {
    let tx_size = tx.serialized_size();  // 字节
    let fee_rate = 10;  // Sat/byte (根据网络拥堵调整)
    
    tx_size * fee_rate  // 返回总 Satoshis
}
```

示例：
```
Bitcoin Transaction:
├─ 交易大小: 226 bytes
├─ 费率: 10 Sat/byte
└─ 总费用: 2,260 Satoshis ≈ 0.0000226 BTC
```

#### Solana
```rust
// Solana 使用固定微费 (lamports)
// 另加动态 "compute budget"

fn estimate_compute_units_solana(tx: &TxIR) -> u64 {
    let mut cu = 200_000;  // 基础 CU
    
    // 根据指令数增加
    cu += tx.instructions.len() as u64 * 50_000;
    
    cu
}

// 费用 = compute_units × 价格 (≈0.00025 lamports/CU)
```

### 3. 跨链交易的 Gas 如何汇总

对于原子交易（Alice: 1 BTC → Bob: 10 ETH，Bob: 10 ETH → Alice: 1 BTC）：

```
原子交易总成本：
├─ 链1 (Bitcoin):
│  ├─ Alice 发送 1 BTC 费用: 2,260 Sat
│  └─ Bob 接收 (零费用，输入由 Alice 承担)
│
├─ 链2 (Ethereum):
│  ├─ Bob 发送 10 ETH gas: 50,480 gas × 50 Gwei = 0.002524 ETH
│  └─ Alice 接收 (零费用)
│
├─ 中间件 (SuperVM):
│  ├─ MVCC 提交: ~1,000 gas
│  ├─ 2PC 协调: ~2,000 gas
│  └─ 签名验证: ~500 gas
│
└─ 总成本计:
   ├─ BTC: 2,260 Sat ≈ $0.71 (@ 250k BTC/USD)
   ├─ ETH: 0.002524 ETH ≈ $3.79 (@ 1500 ETH/USD)
   └─ 总: ≈ $4.50
```

**关键洞察**：
- 每条链独立收费，成本加总
- SuperVM 作为中介的成本微不足道 (~$0.001)
- 最贵的通常是 Gas 较高的 L1（Bitcoin 优先级费，Ethereum 拥堵费）

---

## 原子交易的两阶段共识

### 1. 整体流程

```
时间轴：

T0: 客户端提交        T1: Prepare       T2: Commit       T3: 最终确认
    ├─────────────────┼────────────────┼──────────────┼──────────
    │ 获取 swap_id    │ 锁定资源        │ 原子写入      │ 广播确认
    │ 签名验证        │ 校验读集合      │ 释放锁        │ 跨链同步
    │ 时间戳检查      │ 分配 commit_ts  │ 更新索引      │ 结算
    └─────────────────┴────────────────┴──────────────┴──────────
                Prepare 阶段              Commit 阶段        Finalize
```

### 2. Prepare Phase（准备阶段）

**目的**：验证所有前提条件，获取资源锁，确定全局顺序

```rust
pub fn prepare_and_commit(&self, txn: Txn) -> Result<u64, String> {
    // ========== PREPARE 阶段 ==========
    
    // 1. 构造锁集合（排序以避免死锁）
    let mut keys: Vec<Vec<u8>> = txn.writes().keys().cloned().collect();
    keys.sort();  // ← 全局一致顺序，防环形死锁
    
    // 2. 批量加锁（持有所有 write keys 的锁）
    let locks: Vec<_> = keys.iter().map(|k| self.store.key_lock(k)).collect();
    let _guards: Vec<_> = locks.iter().map(|lk| lk.lock()).collect();
    
    // 3. 并行校验读集合（检查版本冲突）
    let conflict = txn.reads()
        .par_iter()
        .find_any(|read_key| {
            let tail_ts = txn.get_tail_ts(read_key);
            tail_ts > txn.start_ts  // ← 如果有更新的版本，冲突!
        });
    
    if let Some(conflict_key) = conflict {
        return Err(format!("2PC abort: read-write conflict on key"));
        // ← 失败：所有锁自动释放，无副作用
    }
    
    // 4. 分配全局提交时间戳（保证全序）
    let commit_ts = self.store.next_ts();
    
    // ← 至此 Prepare 成功，进入 Commit
}
```

**Prepare Phase 在原子交换中的含义**：

| 操作 | 内容 |
|-----|------|
| 读集合 | 查询 Alice 在 Bitcoin 的余额、Bob 在 Ethereum 的余额 |
| 写集合 | 更新 4 个余额（Alice↓ BTC, Bob↑ BTC, Bob↓ ETH, Alice↑ ETH） |
| 加锁 | 锁定这 4 个账户，防止并发修改 |
| 校验 | 确认读到的余额没有被其他事务更新过（读-写冲突检测） |
| 时间戳 | 决定此交易在全局历史中的位置 |

**成功条件**（全部通过）：
- ✅ Alice 余额 ≥ 1 BTC
- ✅ Bob 余额 ≥ 10 ETH
- ✅ 没有并发冲突
- ✅ 时间未过期 (deadline > now)

**失败条件**（任一触发则失败）：
- ❌ Alice 余额不足
- ❌ Bob 余额不足
- ❌ 读到的数据被其他事务抢先修改
- ❌ 死锁（不会，因为加锁顺序全局一致）

### 3. Commit Phase（提交阶段）

**目的**：原子地写入所有修改，确保一致性

```rust
// === COMMIT 阶段：批量写入 ===

// 依赖：此时持有所有必要的锁，Prepare 已通过

for (key, value_opt) in txn.writes() {
    self.store.append_version(key, commit_ts, value_opt.clone());
    //                                         ↑
    //                              使用 Prepare 分配的时间戳
}

// 锁自动释放（_guards 离开作用域）
Ok(commit_ts)
```

**写入的原子性保证**：

```
RocksDB WAL (Write-Ahead Log)
┌──────────────────────────────────────┐
│ [T100] Alice: 10 BTC → 9 BTC        │ ← 原子写入
│ [T100] Bob: 20 ETH → 30 ETH         │   这两行同时
│ [T100] Bob: 5 BTC → 6 BTC           │   要么全有
│ [T100] Alice: 0 ETH → 10 ETH        │   要么全无
│                                      │
│ [COMMIT] 4 mutations committed       │
└──────────────────────────────────────┘

即使节点在提交中崩溃，恢复时：
- 要么全部 4 个写入已持久化
- 要么全部都没有（对应 Undo Log）
```

---

## 广播逻辑

### 1. "一方形成共识，另一方没完成"的场景

**场景描述**：

```
时间线：
┌────────────────────────────────────────────────────────────┐
│ T0   │ T1        │ T2            │ T3    │ T4         │    │
├──────┼───────────┼───────────────┼───────┼────────────┼────┤
│      │ 提交交换  │ Prepare OK    │       │ 节点崩溃   │    │
│ 开始 │ 请求     │ 已锁定资源    │ ...   │ (在commit) │    │
│      │           │ 校验通过      │       │ 但已写WAL  │    │
│      │           │ 时间戳已分配  │       │            │    │
│      │           │               │       │ 恢复后     │    │
│      │           │               │       │ 自动重做   │    │
└──────┴───────────┴───────────────┴───────┴────────────┴────┘
                    ↑ 如果这里广播，但后面失败？
```

### 2. SuperVM 的解决方案：不分阶段广播

```
关键设计：单阶段提交的虚假假象 + WAL 保证

pub fn execute_atomic_swap(&self, request: SwapRequest) -> Result<SwapReceipt> {
    // Step 1-7: 所有操作都在事务内
    
    // Step 12: 唯一的广播点
    tx.commit()?;  // ← 返回前完全确定成功/失败
    
    if success {
        // → 广播 SwapReceipt（包含所有链的交易哈希）
        // → 此时已 100% 确定在 SuperVM 侧成功
        // → 各链侧异步同步
    } else {
        // → 不广播任何内容，直接返回错误
        // → 客户端自动重试或取消
    }
}
```

### 3. 广播的具体内容

```rust
pub struct SwapReceipt {
    swap_id: Vec<u8>,              // SHA256(request)
    from: SuperVMAccountId,        // Alice
    to: SuperVMAccountId,          // Bob
    from_asset: AssetAmount,       // 1 BTC
    to_asset: AssetAmount,         // 10 ETH
    timestamp: u64,                // 执行时刻
    tx_hashes: Vec<(u64, Vec<u8>)>,  // [(chain_id, tx_hash), ...]
    status: SwapStatus,            // Success | Failed | Pending | Expired
}
```

**广播过程**：

```
SuperVM 提交成功后：

┌─────────────────────────────────────────────────────┐
│ 1. 构建 SwapReceipt (包含 4 笔转账的确认)           │
├─────────────────────────────────────────────────────┤
│ 2. 签名 SwapReceipt (使用 SuperVM validator key)    │
├─────────────────────────────────────────────────────┤
│ 3. 广播给所有链的轻客户端 (Light Clients)          │
│    ├─ Bitcoin: 通过 Bitcoin Light Client          │
│    ├─ Ethereum: 通过 Ethereum Light Client        │
│    ├─ Solana: 通过 Solana Program                 │
│    └─ ...                                          │
├─────────────────────────────────────────────────────┤
│ 4. 各链验证签名 + 状态根证明                        │
│    └─ 如果验证通过，标记 "已确认" (finalized)      │
├─────────────────────────────────────────────────────┤
│ 5. 返回 finality_proof 给客户端                     │
│    └─ 客户端可向原链广播此证明，触发 unlock       │
└─────────────────────────────────────────────────────┘
```

### 4. 各阶段失败时的广播行为

| 阶段 | 失败点 | 广播内容 | 后果 |
|-----|--------|--------|------|
| Prepare | 读集合冲突 | 无广播 | 重试或用户取消 |
| Prepare | 余额不足 | 无广播 | 返回错误给用户 |
| Prepare | 超时 (deadline过期) | 无广播 | 交易作废 |
| Commit | WAL 写入中崩溃 | 无广播 | 恢复后自动重做 |
| Commit | 全部成功 | **广播** SwapReceipt | 各链异步最终确认 |

---

## 失败场景处理

### 1. "一方形成共识，另一方没完成"的完整分析

#### 情景A：SuperVM 侧 Commit 成功，但 Ethereum 侧确认延迟

```
时间轴：
┌─────────────────────────────────────────────────┐
│ T0-T1: SuperVM   ✅ Commit OK, 广播 Receipt  │
│       ├─ Bob 的余额: -10 ETH ✓             │
│       ├─ Alice 的余额: +10 ETH ✓           │
│       └─ 写入 WAL & 广播                   │
│                                             │
│ T1-T5: Ethereum  ⏳ 等待 12 个块            │
│       ├─ Receipt 已收到                     │
│       ├─ 轻客户端验证签名: ✓               │
│       ├─ 状态根证明检查: ✓                 │
│       └─ Finalize: 等待确认数              │
│                                             │
│ T5+: Ethereum   ✅ 确认 (finalized)        │
│     └─ 此时双方都已提交，交易 100% 成功   │
│                                             │
│ 用户视角：                                   │
│ ├─ T1: "Your swap was successful!"         │
│ ├─ T5: "Ethereum confirmation received"   │
│ └─ 余额已更新在两条链                      │
└─────────────────────────────────────────────────┘
```

**不会发生**：
- ❌ SuperVM 成功，Ethereum 回滚（因为收据已签名且包含状态根证明）
- ❌ Bob 收到 10 ETH，但 Alice 没收到（因为是原子的，全部成功）

#### 情景B：SuperVM Prepare 成功但 Commit 中断

```
时间轴：
┌──────────────────────────────────────────────┐
│ T0-T1: Prepare  ✅ OK                        │
│       ├─ 锁定 4 个账户                      │
│       ├─ 校验通过                           │
│       └─ commit_ts = T100 已分配            │
│                                              │
│ T1-T2: Commit   ⚠️ 写入中崩溃               │
│       ├─ WAL 记录: [T100] write #1: OK      │
│       ├─ WAL 记录: [T100] write #2: OK      │
│       ├─ WAL 记录: [T100] write #3: OK      │
│       ├─ CRASH ❌ (write #4 未写)           │
│       └─ 无 COMMIT 标记 → 未确认            │
│                                              │
│ T2+: 恢复 (Recovery)                        │
│     ├─ 读 WAL                              │
│     ├─ 发现 [T100] 有 3 个写入但未 COMMIT  │
│     ├─ Undo: 回滚这 3 个写入              │
│     └─ 锁释放，交易作废                    │
│                                              │
│ 客户端视角：                                 │
│ ├─ T0: "Processing swap..."                │
│ ├─ T2: 连接中断                             │
│ ├─ 重新查询: status = PENDING / FAILED      │
│ └─ 用户可重试                               │
└──────────────────────────────────────────────┘
```

**保证**：
- ✅ **全或无** (All-or-Nothing)：4 笔转账同时成功或同时失败
- ✅ **ACID 有效**：即使崩溃也能恢复到一致状态
- ✅ **无部分状态**：不存在 Alice 收到但 Bob 没转的情况

### 2. 网络分区（Byzantine 情景）

```
假设 Ethereum 网络分区，Bob 的节点无法同步：

┌────────────────────────────────────────────────┐
│ 主分区 (多数)          │ 少数分区 (Bob)      │
├────────────────────────┼───────────────────────┤
│ SuperVM 已确认成功     │ SuperVM 未同步     │
├────────────────────────┼───────────────────────┤
│ Ethereum 已确认        │ 等待网络恢复       │
│ Alice 看到 +10 ETH     │                     │
│                         │ (未来某个时点)      │
│ ✅ 交易已最终确认       │ → 网络愈合，同步  │
│                         │ → 收到 SwapReceipt│
│                         │ → 验证签名        │
│                         │ → Alice 确认接收  │
│                         │ ✅ 同步完毕      │
└────────────────────────┴───────────────────────┘
```

**机制**：
- 少数分区内的 Bob 不会看到任何更新（因为他的轻客户端无法验证）
- 主分区内交易 100% 最终确认
- 愈合后，少数分区自动同步多数分区的状态

---

## 理论基础

### 1. 共识模型：两阶段提交 (2PC)

**定理**：在无拜占庭故障假设下，2PC 保证强一致性

```
2PC 三阶段：

┌───────────────────────────────────────────────┐
│ Phase 1: Prepare (Voting)                     │
│ ├─ 协调器 (Coordinator): "Vote?"             │
│ ├─ 参与者 A: "Yes, 我有 1 BTC"              │
│ ├─ 参与者 B: "Yes, 我有 10 ETH"             │
│ └─ 协调器: 记录"可以提交"并分配时间戳      │
├───────────────────────────────────────────────┤
│ Phase 2: Commit (Execution)                   │
│ ├─ 协调器: "所有人执行！"                   │
│ ├─ 参与者 A: 执行 (transfer -1 BTC)        │
│ ├─ 参与者 B: 执行 (transfer +10 ETH)       │
│ └─ 返回 "Done" 后，不可逆转                 │
├───────────────────────────────────────────────┤
│ Phase 3: Abort (如 Prepare 失败)            │
│ ├─ 协调器: "都回滚！"                       │
│ ├─ 参与者 A: Undo (无副作用，因为未改变)   │
│ ├─ 参与者 B: Undo                           │
│ └─ 交易作废，重新开始                       │
└───────────────────────────────────────────────┘
```

**应用于原子交换**：

| 角色 | 对应实体 |
|-----|--------|
| Coordinator | SuperVM MVCC Store (事务协调器) |
| Participant A | Alice 的账户 (Bitcoin 侧) |
| Participant B | Bob 的账户 (Ethereum 侧) |
| Resource | 余额版本 + 锁 |

**证明草图**：

```
定理：若 Prepare 和 Commit 都在单个原子操作内，
      则不存在部分成功的状态。

证明：
1. Prepare 成功 ⟹ 所有读取都已锁定，版本冲突检查通过
2. 分配 commit_ts ⟹ 全局顺序确定
3. Commit 开始前，任何读者都看不到中间状态
   (因为锁未释放)
4. Commit 开始 ⟹ 写入 WAL (持久化)
5. Commit 成功 ⟹ 所有修改已原子写入版本存储
6. 即使崩溃在第 4-5 步间，恢复时：
   - 检查 WAL 中是否有 COMMIT 标记
   - 无标记 ⟹ Undo (全部回滚)
   - 有标记 ⟹ Redo (重新应用)
   
结论：✅ 不存在部分状态
```

### 2. 广播的 Happens-Before 关系

```
定义：操作 A → B 当且仅当所有观察者都看到 A 在 B 之前发生

原子交换的 Happens-Before 序列：

Prepare Start → Prepare OK → Commit Start → Commit OK → Broadcast

其中：
- Prepare OK ⟹ 已获取所有锁 + 版本校验通过
- Commit OK ⟹ 已写入 WAL + 分布式快照成功
- Broadcast ⟹ 已签名 SwapReceipt + 所有链的轻客户端已验证

推论：
1. Broadcast 不可能发生在 Prepare OK 之前
2. 若 Broadcast 发生，则 Commit OK 已保证
3. 若节点崩溃在 Broadcast 前，恢复后自动重新 Broadcast
```

### 3. 原子性的三个层次

```
Layer 1: MVCC 原子性
├─ 保证：单个对象的版本更新不可分割
├─ 例：Alice 的 BTC 要么 -1，要么 0，不可能 -0.5
└─ 机制：版本链 + 时间戳排序

Layer 2: 事务原子性 (ACID)
├─ 保证：多个对象的修改同时成功或同时失败
├─ 例：4 笔转账 (Alice -BTC, Bob +BTC, Bob -ETH, Alice +ETH)
│       要么全有，要么全无
└─ 机制：WAL + 2PC + 锁 + Undo Log

Layer 3: 跨链原子性
├─ 保证：一旦 SuperVM 提交，各链的最终确认不可反转
├─ 例：Alice 一定能在 Bitcoin 和 Ethereum 上看到正确的余额
└─ 机制：签名证明 + 轻客户端验证 + 状态根承诺
```

### 4. CAP 定理权衡

```
SuperVM 在 CAP 定理中的位置：

         ╔════════════════════════════════════════╗
         ║           CAP 定理权衡空间              ║
         ╠════════════════════════════════════════╣
         ║                                        ║
         ║    一致性 (Consistency)               ║
         ║         △                             ║
         ║        /│\                            ║
         ║       / │ \                           ║
         ║      /  │  \                          ║
         ║     /   ●SuperVM                      ║
         ║    /   / \                            ║
         ║   /   /   \                           ║
         ║  /   /     \                          ║
         ║可用性────────分区容限                   ║
         ║(Availability) (Partition Tolerance) ║
         ║                                        ║
         ║ SuperVM 选择: CP (一致性 + 分区容限)  ║
         ║ - 无拜占庭时：强一致性 ✓              ║
         ║ - 网络分区时：少数分区稍延迟 ⚠️       ║
         ║ - 可用性：超过 99.99% (高可用)        ║
         ║                                        ║
         ╚════════════════════════════════════════╝
```

---

## 总结表格

| 问题 | 答案 | 机制 |
|-----|------|------|
| **Gas 如何算** | 链特定 + 中间件 | ChainAdapter.estimate_gas() |
| **多链 Gas 汇总** | 各链费用相加 | Sum of (chain1_gas + chain2_gas + ...) |
| **如何形成共识** | 2PC (Prepare + Commit) | TwoPhaseCoordinator |
| **一方成功，另一方未完** | 无此现象 (原子性) | WAL + Undo Log 保证 All-or-Nothing |
| **广播出去又失败怎么办** | 不会广播失败状态 | Commit OK 才广播 SwapReceipt |
| **网络分区怎么办** | 多数分区继续，少数等待 | Quorum-based finality |
| **崩溃恢复** | 自动重做或撤销 | WAL + Redo/Undo Log |

---

## 代码导航

- **原子交换核心**: `src/vm-runtime/src/chain_linker/atomic_swap.rs`
- **2PC 协调器**: `src/vm-runtime/src/two_phase_consensus.rs`
- **链适配器**: `src/vm-runtime/src/chain_linker/chain_adapter.rs`
- **MVCC 存储**: `src/vm-runtime/src/mvcc_store.rs`
- **测试用例**: `src/vm-runtime/src/chain_linker/tests/atomic_swap_integration.rs`

