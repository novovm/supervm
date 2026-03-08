# WEB30 协议族参考实现

本目录包含 WEB30 协议族的完整参考实现，涵盖核心代币标准、身份系统、DEX、消息协议、**SuperVM 主链 Token 经济模型**等。

## 🆕 SuperVM 主链 Token 经济模型

完整的去中心化经济系统设计,包含双轨收入、外汇储备、分红机制、治理结构等:

- **[快速参考](TOKEN-QUICKREF.md)** - 核心数据与机制一览
- **[技术实现](TOKEN-IMPLEMENTATION.md)** - 代码结构与实施说明
- **[完整商业计划](../docs/商业计划/经济模型与治理.md)** - 9章详细文档
- **[ROADMAP](../../ROADMAP.md)** - Phase T1-T4 实施路线

**核心特性**:
- 💰 双轨收入: 本币(80%分红+20%国库) + 外币(100%储备)
- 🌍 外汇储备: BTC/ETH/USDT等多币种AMM池
- 💎 每日分红: 主动领取,避免Gas浪费
- 🏛️ 加权治理: 9人议会(创始人35%+Top5持币者50%+团队10%+独立5%)
- 🔥 持续通缩: 年化-3%~-5%销毁
- 📈 价值增长: 资源锚定+多资产背书+网络效应

## 目录结构

```
web30/
├── README.md                      # 本文件
├── TOKEN-QUICKREF.md              # 🆕 Token经济快速参考
├── TOKEN-IMPLEMENTATION.md        # 🆕 Token技术实现文档
├── MainnetToken.sol               # 🆕 主链Token Solidity接口
├── core/                          # 核心合约
│   ├── src/
│   │   ├── token.rs               # WEB30 Token (Rust/WASM)
│   │   ├── mainnet_token.rs       # 🆕 主链Token接口
│   │   ├── treasury.rs            # 🆕 国库管理
│   │   ├── foreign_payment.rs     # 🆕 外币支付(600+行)
│   │   ├── dividend_pool.rs       # 🆕 分红池(330+行)
│   │   ├── governance.rs          # 🆕 治理模块(550+行)
│   │   └── lib.rs                 # 模块导出
│   ├── WEB30Token.sol             # WEB30 Token (Solidity)
│   └── tests/                     # 单元测试
├── identity/                      # WEB3005 身份与信誉
│   ├── identity.rs                # 统一账户/钱包绑定
│   ├── kyc.rs                     # KYC 证明与验证
│   └── WEB3005Identity.sol        # Solidity 版本
├── dex/                           # WEB3009 DEX
│   ├── orderbook.rs               # 订单簿
│   ├── matching.rs                # 撮合引擎
│   └── WEB3009DEX.sol             # Solidity 版本
├── messaging/                     # WEB3014 消息协议
│   ├── p2p.rs                     # P2P 消息层
│   └── anchor.rs                  # 历史锚定
├── sdk/                           # TypeScript SDK
│   ├── package.json
│   ├── src/
│   │   ├── web30.ts               # WEB30 Token SDK
│   │   ├── web3005.ts             # 身份/KYC SDK
│   │   ├── web3009.ts             # DEX SDK
│   │   └── web3014.ts             # 消息 SDK
│   └── examples/                  # 使用示例
└── examples/                      # DApp 示例
    ├── simple-transfer/           # 简单转账
    ├── kyc-gated-app/             # KYC 门控应用
    └── dex-demo/                  # DEX 演示
```

## 快速开始

### 部署 SuperVM 主链 Token

#### Rust/WASM 版本 (主网原生)
```bash
cd contracts/web30/core
cargo build --target wasm32-unknown-unknown --release
supervm deploy --wasm target/wasm32-unknown-unknown/release/mainnet_token.wasm
```

#### Solidity 版本 (EVM 兼容层)
```bash
cd contracts/web30
solc --bin --abi MainnetToken.sol -o build/
# 部署到 Ethereum/Polygon/BSC 等 EVM 链作为镜像资产
```

### 部署 WEB30 Token 合约

#### Rust/WASM 版本
```bash
cd contracts/web30/core
cargo build --target wasm32-unknown-unknown --release
supervm deploy --wasm target/wasm32-unknown-unknown/release/web30_token.wasm
```

#### Solidity 版本 (EVM 兼容链)
```bash
cd contracts/web30/core
forge build
forge deploy WEB30Token --constructor-args "SuperVM Token" "SVM" 18
```

### 使用 TypeScript SDK

```bash
cd contracts/web30/sdk
npm install
npm run build
```

```typescript
import { WEB30Client } from '@supervm/web30';

const client = new WEB30Client({ rpcUrl: 'http://localhost:8545' });
const token = await client.getToken('0x...');

// 转账
const receipt = await token.transfer('0xRecipient...', '1000000000000000000');

// 跨链转账
const crossChainReceipt = await token.transferCrossChain(
  137, // Polygon
  '0xRecipient...',
  '1000000000000000000'
);
```

## 协议实现状态

| 协议 | Rust/WASM | Solidity | SDK | 测试 | 状态 |
|------|-----------|----------|-----|------|------|
| WEB30 Token | ✅ | ✅ | ✅ | ✅ | 完成 |
| WEB3005 Identity | ✅ | ✅ | ✅ | ✅ | 完成 |
| WEB3009 DEX | ✅ | ✅ | ✅ | ✅ | 完成 |
| WEB3014 Messaging | ✅ | 🚧 | ✅ | ✅ | 进行中 |
| WEB3001 NFT | 📋 | 📋 | 📋 | 📋 | 规划中 |
| WEB3002 Multi-Token | 📋 | 📋 | 📋 | 📋 | 规划中 |

## 架构特性

### MVCC 并行执行
所有 Rust 合约利用 SuperVM 的 MVCC 引擎实现真正的并行执行：
- 自动冲突检测（读写集分析）
- 乐观并发控制
- 495K+ TPS 性能

### 跨链原生支持
集成 `AtomicCrossChainSwap` 执行器：
- 原子跨链转账
- 多链资产统一视图
- 无需第三方桥接

### 隐私保护
集成 RingCT 与 zkVM：
- 隐私转账（环签名）
- 零知识 KYC 证明
- 选择性披露

### 多链兼容
- Solidity 版本兼容 EVM 链（Ethereum/BSC/Polygon）
- SPL Token 适配器（Solana）
- Move 模块适配器（Sui/Aptos）

## 开发指南

### 添加新协议

1. 在 `standards/` 目录创建协议规范
2. 在 `contracts/web30/<protocol>/` 实现核心合约
3. 在 `sdk/src/` 添加 TypeScript 封装
4. 在 `examples/` 添加使用示例
5. 编写测试并更新文档

### 运行测试

```bash
# Rust 单元测试
cargo test -p web30-contracts

# Solidity 测试
forge test

# SDK 测试
cd sdk && npm test

# 集成测试
cargo test --test integration
```

## 贡献

欢迎贡献！请阅读 [CONTRIBUTING.md](../../CONTRIBUTING.md) 了解详情。

## 许可证

MIT License - 详见 [LICENSE](../../LICENSE)
