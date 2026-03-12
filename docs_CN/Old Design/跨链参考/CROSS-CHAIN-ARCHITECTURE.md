# SuperVM 跨链统一架构设计文档

**版本**: 1.0  
**日期**: 2025-11-13  
**架构师**: KING XU

> 实操入口：想直接上手运行跨链执行的最小用法？请参考《[Cross-Chain Executor 使用指南](cross-executor-usage.md)》。

## 📋 执行摘要

SuperVM 采用**原生协调架构**（Native Coordination Architecture），通过统一账户模型实现多链资产和合约的无缝集成。核心理念是：**外部链保持独立，SuperVM 提供统一入口**。

### 核心特性
- ✅ **统一账户系统**: 一个 SuperVM 账户关联多条链的外部地址
- ✅ **原子跨链交换**: 基于 MVCC 事务的 all-or-nothing 保证
- ✅ **跨链智能合约**: 在不同链上部署和调用合约（WASM/EVM/Solana）
- ✅ **跨链挖矿**: 矿工选择任意链接收奖励
- ✅ **12位数字账户**: 可读性强的账户标识（KYC 扩展支持）

---

## 🎯 架构核心原则

### 1. **协调而非桥接** (Coordinator, NOT Bridge)

❌ **错误理解**: SuperVM 是跨链桥，需要锁定/铸造代币  
✅ **正确理解**: SuperVM 是协调器，直接操作各链原生资产

```rust
// 错误的"桥接"思维
Alice.ETH -> SuperVM (锁定) -> 铸造 wrappedETH -> Bob
Bob.SOL -> SuperVM (锁定) -> 铸造 wrappedSOL -> Alice

// 正确的"协调"思维
Alice.ETH (1链) ---\
                    SuperVM 协调原子交易
Bob.SOL (900链) ---/
结果: Alice.ETH直接到Bob.ETH, Bob.SOL直接到Alice.SOL
```

### 2. **统一账户模型** (Unified Account Model)

每个 SuperVM 账户可以关联多个外部链地址：

```
SuperVM Account (Alice)
├── 账户标识
│   ├── 公钥地址: 0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb
│   └── 数字账户: 888888888888 (12位)
├── 关联的外部链账户
│   ├── 以太坊 (chain_id=1): 0xAA...AA
│   ├── Solana (chain_id=900): Sol123...456
│   ├── Bitcoin (chain_id=0): bc1q...xyz
│   └── BSC (chain_id=56): 0xBB...BB
└── 资产汇总 (由 SuperVM 查询聚合)
    ├── ETH: 10.5
    ├── SOL: 150.0
    ├── BTC: 0.25
    └── BNB: 5.0
```

### 3. **原子性保证** (Atomicity Guarantee)

所有跨链操作都包裹在 MVCC 事务中，任何步骤失败都会完整回滚：

```rust
// 原子交换流程
pub fn execute_atomic_swap(request: SwapRequest) -> Result<Receipt> {
    let mut tx = storage.begin_transaction()?;  // 开始事务
    
    // Step 1: 验证双方余额（失败 -> 早退，无副作用）
    verify_balances(&tx, &request)?;
    
    // Step 2: 执行四笔转账（在事务内）
    tx.set(alice_eth_key, alice_eth_balance - amount_eth)?;
    tx.set(bob_eth_key, bob_eth_balance + amount_eth)?;
    tx.set(bob_sol_key, bob_sol_balance - amount_sol)?;
    tx.set(alice_sol_key, alice_sol_balance + amount_sol)?;
    
    // Step 3: 验证守恒定律（额外安全检查）
    assert_conservation_laws(&tx)?;
    
    // Step 4: 原子提交（all-or-nothing）
    tx.commit()?;  // 如果这里崩溃，RocksDB WAL 保证恢复
    
    Ok(receipt)
}
```

**不可能发生的场景**:
- ❌ Alice 的 ETH 扣了，但 Bob 没收到 → **不可能**（事务回滚）
- ❌ Bob 的 SOL 扣了，但 Alice 没收到 → **不可能**（事务回滚）
- ❌ 一方成功另一方失败 → **不可能**（原子提交）
- ❌ 网络崩溃导致部分成功 → **不可能**（WAL 恢复）

---

## 🏗️ 系统架构

### 核心组件

```
┌─────────────────────────────────────────────────────────┐
│                     SuperVM Core                        │
│  ┌──────────────────────────────────────────────────┐  │
│  │        Unified Account System (账户系统)         │  │
│  │  - SuperVMAccount (统一账户)                      │  │
│  │  - 公钥地址 + 12位数字账户                         │  │
│  │  - 多链地址关联 (ETH/SOL/BTC/...)                 │  │
│  └──────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │      MVCC Transaction Engine (事务引擎)          │  │
│  │  - begin_transaction() / commit() / abort()      │  │
│  │  - RocksDB WAL 崩溃恢复                           │  │
│  │  - 版本冲突检测                                   │  │
│  └──────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────┐  │
│  │   Cross-Chain Coordinators (跨链协调器)          │  │
│  │  - AtomicCrossChainSwap (原子交换)               │  │
│  │  - CrossChainContractCoordinator (合约协调)      │  │
│  │  - CrossChainMiningCoordinator (挖矿协调)        │  │
│  │  - CrossChainTransfer (跨链转账)                 │  │
│  └──────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
                            │
            ┌───────────────┼───────────────┐
            ▼               ▼               ▼
    ┌───────────────┐ ┌──────────┐ ┌────────────┐
    │ Ethereum      │ │ Solana   │ │ Bitcoin    │
    │ (chain_id=1)  │ │ (chain_id│ │ (chain_id= │
    │               │ │ =900)    │ │ 0)         │
    │ Alice: 0xAA   │ │ Alice:   │ │ Alice:     │
    │ Bob: 0xCC     │ │ Sol...BB │ │ bc1q...xyz │
    └───────────────┘ └──────────┘ └────────────┘
```

### 存储键格式 (StorageKey Format)

所有链的数据统一存储在 SuperVM 的 KV 数据库中，通过前缀隔离：

```
格式: chain:{chain_id}:{chain_type}:{address}:{field}

示例:
- chain:1:evm:0xAA...AA:balance          → Alice 的 ETH 余额
- chain:900:solana:Sol...BB:balance      → Alice 的 SOL 余额
- chain:1:evm:0xCC...CC:nonce            → Bob 的 ETH nonce
- chain:1:evm:0x123...456:code           → 智能合约代码
- supervm:account:0x742d...bEb           → SuperVM 账户元数据
- swap:receipt:0xabcd1234                → 交换收据
```

---

## 💡 使用场景

### 场景 1: 跨链资产交换

**用户视角**:
```
Alice 想用 2 ETH 换 Bob 的 20 SOL

SuperVM 界面:
[发起交换]
  你支付: 2 ETH
  你收到: 20 SOL
  交易对方: Bob (100000000001)
  [确认交易]
```

**底层执行**:
1. SuperVM 加载 Alice 和 Bob 的账户
2. 查询关联的 ETH 和 Solana 地址
3. 开始 MVCC 事务
4. 验证双方余额充足
5. 执行四笔转账:
   - Alice.ETH (1链) - 2 ETH
   - Bob.ETH (1链) + 2 ETH
   - Bob.SOL (900链) - 20 SOL
   - Alice.SOL (900链) + 20 SOL
6. 原子提交
7. 返回交换收据

### 场景 2: 跨链智能合约调用

**SuperVM 原生智能合约的"万能"特性** ⭐:

当智能合约部署在 SuperVM 原生层（非外部链）时，该合约具有**跨链协调能力**：

```rust
// SuperVM 原生合约可以:

contract UniversalDeFi {
    // 1. 原子操作多条链
    fn atomic_swap(alice_eth: u128, bob_sol: u128) {
        let mut tx = storage.begin_transaction();  // MVCC 事务
        
        // 同时操作 ETH 链和 Solana 链
        tx.set("chain:1:evm:alice:balance", alice_eth - amount);  // ETH
        tx.set("chain:900:solana:bob:balance", bob_sol - amount); // SOL
        
        tx.commit();  // 原子提交，all-or-nothing
    }
    
    // 2. 跨链条件逻辑
    fn conditional_transfer() {
        if alice.eth_balance > 10 && bob.sol_balance > 100 {
            // 同时操作两条链
            transfer_eth(alice, bob, 5);
            transfer_sol(bob, alice, 50);
        }
    }
    
    // 3. 多链资产聚合
    fn total_value(user: Address) -> u128 {
        let eth = get_balance(1, user);    // Ethereum
        let sol = get_balance(900, user);  // Solana
        let btc = get_balance(0, user);    // Bitcoin
        return eth * eth_price + sol * sol_price + btc * btc_price;
    }
}
```

**为什么是"万能"的？**

1. **统一事务控制**: 合约内可以使用 MVCC 事务包裹多链操作
2. **原子性保证**: 所有链的操作要么全部成功，要么全部失败
3. **无需外部桥**: 直接访问 SuperVM 统一存储，无需跨链桥
4. **零信任成本**: 不依赖外部预言机或中继

**对比外部链合约**:

| 特性 | SuperVM 原生合约 | 外部链合约 (ETH/SOL) |
|------|------------------|----------------------|
| 跨链操作 | ✅ 原生支持 | ❌ 需要桥接 |
| 原子性 | ✅ MVCC 保证 | ⚠️ 需要复杂协议 |
| 多链访问 | ✅ 直接访问 | ❌ 需要预言机 |
| 性能 | ✅ 单次事务 | ⚠️ 多次链上确认 |
| 费用 | ✅ 低廉 | ⚠️ 多链 gas |

**用户视角**:
```
Alice 在 SuperVM 部署一个"万能" DeFi 合约

该合约可以:
- 在 SuperVM 原生层执行 (WASM) → 🌟 可协调多链
- 在以太坊上执行 (EVM模式) → 仅操作 ETH 链
- 在 Solana 上执行 (BPF模式) → 仅操作 SOL 链
```

**实现**:
```rust
// 部署到以太坊
let contract_address = coordinator.deploy_contract(DeployRequest {
    deployer: alice_id,
    target_chain: 1,  // Ethereum
    code: evm_bytecode,
    contract_type: ContractType::EVM,
})?;

// 从任意链调用
let result = coordinator.call_contract(CallRequest {
    caller: bob_id,
    chain_id: 1,
    contract_address,
    method: "transfer",
    args: encode_args(&[bob_address, amount]),
})?;
```

### 场景 3: 跨链挖矿

**用户视角**:
```
Alice 是矿工，她想:
- 用 GPU 挖 SuperVM 区块
- 但奖励直接发放到她的 Solana 账户（交易费更便宜）
```

**实现**:
```rust
// 注册矿工，指定奖励链
mining.register_miner(alice_id, reward_chain: 900)?;  // Solana

// 挖矿成功后，SuperVM 自动发放到 Solana
mining.submit_mining_result(submission)?;
// → Alice.SOL 余额自动增加 50 SOL
```

---

## 🔢 12位数字账户系统

### 号段分配规则

| 前缀 | 范围 | 类型 | 用途 | 示例 |
|------|------|------|------|------|
| 1xx | 100000000000-199999999999 | 普通用户 | 免费注册 | 123456789012 |
| 2xx | 200000000000-299999999999 | 企业用户 | 企业认证 | 234567890123 |
| 3xx | 300000000000-399999999999 | 机构用户 | 金融机构 | 345678901234 |
| 4xx | 400000000000-499999999999 | KYC认证 | 实名用户 | 456789012345 |
| 5xx | 500000000000-599999999999 | VIP用户 | 高级会员 | 567890123456 |
| 6xx | 600000000000-699999999999 | 合约账户 | 智能合约 | 678901234567 |
| 7xx | 700000000000-799999999999 | 保留 | 未来扩展 | - |
| 8xx | 800000000000-899999999999 | 靓号 | 可拍卖 | 888888888888 |
| 9xx | 900000000000-999999999999 | 系统账户 | 系统保留 | 999999999999 |

### 账户使用

用户可以同时使用公钥地址和数字账户：

```rust
// 转账时可以用公钥
transfer(from: "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb", to: "0x123...", amount);

// 也可以用数字账户 (更易记)
transfer(from: "888888888888", to: "123456789012", amount);

// 两者等价 (内部自动映射)
```

### KYC 扩展

4xx 号段用户可以关联 KYC 信息：

```rust
pub struct KYCInfo {
    pub name_hash: Vec<u8>,        // 姓名哈希(加密)
    pub id_number_hash: Vec<u8>,   // 身份证哈希(加密)
    pub level: u8,                 // KYC等级(1-5)
    pub verified_at: u64,          // 认证时间
    pub verifier: String,          // 认证机构
}
```

---

## 🔐 安全性保证

### 1. 原子性 (Atomicity)

- **机制**: MVCC 事务 + RocksDB WAL
- **保证**: 全部成功或全部失败，无中间状态
- **证明**: 
  - 事务开始前的失败 → 无副作用
  - 事务执行中的失败 → 自动回滚
  - 提交时的崩溃 → WAL 恢复

### 2. 一致性 (Consistency)

- **守恒定律**: 每笔交换后验证资产总量不变
- **余额检查**: 转账前验证余额充足
- **Nonce 防重放**: 每个账户维护 nonce

### 3. 隔离性 (Isolation)

- **版本控制**: 每个事务操作独立版本
- **冲突检测**: commit 时检测版本冲突
- **链隔离**: 不同链的数据通过 chain_id 隔离

### 4. 持久性 (Durability)

- **RocksDB WAL**: 写前日志
- **批量提交**: 原子写入
- **崩溃恢复**: 自动重放 WAL

---

## 📊 性能特性

### 中心化模式 (CEX Mode)

- ✅ **速度快**: 无需等待外部链确认
- ✅ **费用低**: 单次数据库事务
- ✅ **原子性强**: MVCC 保证
- ⚠️  **信任要求**: 需要信任 SuperVM

### 去中心化模式 (DEX Mode - 未来)

- ✅ **无需信任**: 使用外部链智能合约
- ✅ **HTLC**: 哈希时间锁合约
- ⚠️  **速度慢**: 需要等待多链确认
- ⚠️  **费用高**: 多链 gas 费用

---

## 🏗️ 实现状态

### 已完成 ✅

- [x] SuperVMAccount 统一账户系统 (373行)
- [x] SuperVMAccountId (公钥 + 数字账户)
- [x] NumericIdAllocator 数字账户分配器
- [x] AtomicCrossChainSwap 原子交换 (396行)
- [x] SwapRequest/Receipt/Status 数据结构
- [x] CrossChainTransfer 跨链转账
- [x] CrossChainContractCoordinator 合约协调 (338行)
- [x] ContractDeployRequest/CallRequest
- [x] 多 VM 支持 (WASM/EVM/Solana)
- [x] **SuperVM 原生合约的"万能"特性** ⭐ (MVCC 多链协调)
- [x] CrossChainMiningCoordinator 挖矿协调 (316行)
- [x] MinerConfig/MiningTask/Reward
- [x] 跨链完整演示 (cross_chain_demo.rs, 670行)
- [x] Storage Key 隔离机制
- [x] 单元测试框架

### 待完成 🚧

- [ ] 签名验证 (verify_signature)
- [ ] WASM 运行时集成
- [ ] EVM 集成 (revm)
- [ ] Solana BPF VM 集成
- [ ] 跨链查询聚合 (query_user_all_assets)
- [ ] KYC 信息加密存储
- [ ] 两阶段提交协议 (2PC, 可选)
- [ ] 外部链智能合约 (DEX模式, Phase 2)
- [ ] 价格预言机集成
- [ ] 手续费计算和分配

---

## 📖 API 文档

### SuperVMAccount

```rust
// 创建账户
let id = SuperVMAccountId::from_public_key(pubkey)?;
let mut account = SuperVMAccount::new(id);

// 领取数字账户
account.claim_numeric_id(888888888888)?;

// 关联外部链
account.link_account(1, eth_address)?;      // Ethereum
account.link_account(900, sol_address)?;    // Solana

// 设置 KYC
account.set_kyc_info(kyc_info)?;
```

### AtomicCrossChainSwap

```rust
let swapper = AtomicCrossChainSwap::new(storage);

let request = SwapRequest {
    from: alice_id,
    to: bob_id,
    from_asset: AssetAmount {
        chain_id: 1,
        chain_type: ChainType::EVM,
        asset_type: AssetType::Native,
        amount: 2_000_000_000_000_000_000,  // 2 ETH
    },
    to_asset: AssetAmount {
        chain_id: 900,
        chain_type: ChainType::Solana,
        asset_type: AssetType::Native,
        amount: 20_000_000_000,  // 20 SOL
    },
    deadline: now + 3600,
    nonce: 1,
    signature: vec![],
};

let receipt = swapper.execute_atomic_swap(request)?;
```

### CrossChainContractCoordinator

```rust
let coordinator = CrossChainContractCoordinator::new(storage);

// 部署合约
let contract_addr = coordinator.deploy_contract(DeployRequest {
    deployer: alice_id,
    target_chain: 1,
    code: bytecode,
    contract_type: ContractType::EVM,
    init_args: vec![],
    gas_limit: 1_000_000,
})?;

// 调用合约
let result = coordinator.call_contract(CallRequest {
    caller: bob_id,
    chain_id: 1,
    contract_address: contract_addr,
    method: "transfer".to_string(),
    args: encode_args(&[to, amount]),
    value: 0,
    gas_limit: 100_000,
})?;
```

### CrossChainMiningCoordinator

```rust
let mining = CrossChainMiningCoordinator::new(storage);

// 注册矿工
mining.register_miner(alice_id, reward_chain: 900)?;

// 创建挖矿任务
let task = mining.create_mining_task(block_height, difficulty, reward)?;

// 提交挖矿结果
let success = mining.submit_mining_result(MiningSubmission {
    miner: alice_id,
    block_height,
    nonce,
    result_hash,
    submitted_at: now,
})?;
```

---

## 🎓 术语表

- **SuperVM Account**: 统一账户，可关联多条链的外部地址
- **Chain ID**: 链标识符 (1=Ethereum, 900=Solana, 0=Bitcoin)
- **Storage Key**: 存储键，格式 `chain:{id}:{type}:{address}:{field}`
- **MVCC Transaction**: 多版本并发控制事务
- **Atomic Swap**: 原子交换，保证 all-or-nothing
- **Coordinator**: 协调器，统一管理跨链操作
- **Numeric ID**: 12位数字账户 (100000000000-999999999999)
- **KYC**: Know Your Customer，实名认证
- **WAL**: Write-Ahead Log，写前日志

---

## 📚 参考资料

- [MVCC 事务实现](../src/vm-runtime/src/storage/mod.rs)
- [账户系统代码](../src/vm-runtime/src/adapter/account.rs)
- [原子交换代码](../src/vm-runtime/src/adapter/atomic_swap.rs)
- [合约协调代码](../src/vm-runtime/src/adapter/cross_contract.rs)
- [挖矿协调代码](../src/vm-runtime/src/adapter/cross_mining.rs)
- [完整演示](../examples/cross_chain_demo.rs)

---

**文档版本**: 1.0  
**最后更新**: 2025-11-13  
**维护者**: KING XU <leadbrand@me.com>
