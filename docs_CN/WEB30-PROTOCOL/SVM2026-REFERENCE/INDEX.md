# WEB30 协议族参考实现

> 下一代区块链应用层标准 - 完整参考实现

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](../../LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![Solidity](https://img.shields.io/badge/Solidity-0.8.20-blue.svg)](https://soliditylang.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.0+-blue.svg)](https://www.typescriptlang.org/)

## 🎯 什么是 WEB30？

WEB30 是为 SuperVM 生态设计的下一代应用层协议族，提供：

- ✅ **MVCC 并行执行** - 495K+ TPS
- ✅ **跨链原生支持** - 无需第三方桥接
- ✅ **隐私保护** - 环签名 + zkVM
- ✅ **多链兼容** - EVM/Solana/Move 统一接口

## 📁 快速导航

| 文档 | 说明 |
|------|------|
| **[QUICKSTART.md](QUICKSTART.md)** | 🚀 5分钟快速开始 |
| **[IMPLEMENTATION.md](IMPLEMENTATION.md)** | 📖 完整实现指南 |
| **[DELIVERY-REPORT.md](DELIVERY-REPORT.md)** | ✅ 交付物清单与状态 |
| **[sdk/README.md](sdk/README.md)** | 💻 SDK API 文档 |

## 🏗️ 项目结构

```
web30/
├── core/                   # Rust/WASM + Solidity 合约
│   ├── src/                # Rust 源码
│   │   ├── token.rs        # ✅ WEB30 Token
│   │   ├── privacy.rs      # ✅ 隐私功能
│   │   ├── cross_chain.rs  # ✅ 跨链协调
│   │   └── types.rs        # ✅ 数据类型
│   └── WEB30Token.sol      # ✅ Solidity 版本
│
├── sdk/                    # TypeScript SDK
│   ├── src/
│   │   ├── web30.ts        # ✅ Token SDK
│   │   ├── web3005.ts      # ✅ 身份 & KYC SDK
│   │   └── client.ts       # ✅ 统一客户端
│   └── examples/           # ✅ 使用示例
│
└── identity/               # 🚧 WEB3005 实现（规划中）
```

## ⚡ 快速开始

### 1️⃣ 克隆仓库

```bash
git clone https://github.com/XujueKing/SuperVM.git
cd SuperVM/contracts/web30
```

### 2️⃣ 编译 Rust 合约

```bash
cd core
cargo build --release
cargo test  # ✅ 5 passed
```

### 3️⃣ 部署 Solidity 合约

```bash
forge build
forge test
anvil &  # 启动本地测试网

# 部署
forge create WEB30Token \
  --constructor-args "SuperVM Token" "SVM" 18 1000000000000000000000000 \
  --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

### 4️⃣ 使用 SDK

```bash
cd ../sdk
npm install && npm run build

# 运行示例
npx ts-node examples/simple-transfer.ts
```

## 💡 示例代码

### 基础转账

```typescript
import { SuperVMClient, parseTokenAmount } from '@supervm/web30';

const client = new SuperVMClient({
  rpcUrl: 'http://localhost:8545',
  privateKey: process.env.PRIVATE_KEY
});

const token = client.getToken('0xTokenAddress...');

// 转账 100 tokens
const receipt = await token.transfer(
  '0xRecipient...',
  parseTokenAmount('100', 18)
);
console.log('Tx hash:', receipt.txHash);
```

### 跨链转账

```typescript
// 跨链转账到 Polygon
const crossReceipt = await token.transferCrossChain(
  137,  // Polygon chain ID
  '0xRecipient...',
  parseTokenAmount('50', 18)
);
console.log('Swap ID:', crossReceipt.swapId);
```

### KYC 零知识证明

```typescript
// 生成 KYC 证明
const zkProof = await client.identity.proveKycLevel({
  account: myAddress,
  level: 'standard',
  challenge: serverChallenge
});

// 验证证明
const isValid = await client.identity.verifyKycProof({
  proof: zkProof,
  level: 'standard',
  attestors: [{ id: 'did:svm:attestor:bankX', policy: ['standard'] }],
  challenge: serverChallenge
});
```

## 📊 已实现协议

| 协议 | Rust | Solidity | SDK | 状态 |
|------|------|----------|-----|------|
| **WEB30** Token | ✅ | ✅ | ✅ | 完成 |
| **WEB3005** Identity | 🚧 | 🚧 | ✅ | SDK完成 |
| **WEB3009** DEX | 📋 | 📋 | 📋 | 规划中 |
| **WEB3014** Messaging | 📋 | 📋 | 📋 | 规划中 |

## 🧪 测试状态

```bash
# Rust 单元测试
$ cargo test
test result: ok. 5 passed; 0 failed

# Solidity 测试（示例）
$ forge test
[PASS] testTransfer() (gas: 51234)
[PASS] testBatchTransfer() (gas: 98765)
```

## 📚 完整文档

- **架构设计**: [IMPLEMENTATION.md](IMPLEMENTATION.md#架构概览)
- **API 参考**: [sdk/README.md](sdk/README.md)
- **协议规范**: [../../standards/](../../standards/)
- **贡献指南**: [../../CONTRIBUTING.md](../../CONTRIBUTING.md)

## 🛠️ 开发工具

### 必需
- Rust 1.70+
- Foundry (Solidity)
- Node.js 18+

### 推荐
- VS Code + rust-analyzer
- Hardhat/Foundry 扩展
- MetaMask

## 🤝 贡献

欢迎贡献！请阅读 [贡献指南](../../CONTRIBUTING.md)。

常见贡献方向：
- 🐛 修复 bug
- ✨ 添加新协议（WEB3001-WEB3014）
- 📝 改进文档
- 🧪 增加测试覆盖

## 📞 社区

- **GitHub Issues**: [问题反馈](https://github.com/XujueKing/SuperVM/issues)
- **Discord**: (待添加)
- **Twitter**: (待添加)

## 📄 许可证

[MIT License](../../LICENSE)

---

**版本**: v0.1.0  
**最后更新**: 2025-11-17  
**维护者**: SuperVM Team

## ⭐ Star 历史

如果这个项目对你有帮助，请给我们一个 Star！

[![Stargazers over time](https://starchart.cc/XujueKing/SuperVM.svg)](https://starchart.cc/XujueKing/SuperVM)
