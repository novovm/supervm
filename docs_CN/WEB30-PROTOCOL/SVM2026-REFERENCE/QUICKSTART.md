# WEB30 参考实现 - 快速开始指南

## 项目结构

```
contracts/web30/
├── README.md                      # 主文档
├── QUICKSTART.md                  # 本文件
├── core/                          # 核心合约实现
│   ├── Cargo.toml                 # Rust 项目配置
│   ├── src/
│   │   ├── lib.rs                 # 库入口
│   │   ├── token.rs               # WEB30 Token 实现
│   │   ├── types.rs               # 数据类型
│   │   ├── privacy.rs             # 隐私功能
│   │   ├── cross_chain.rs         # 跨链功能
│   │   └── tests.rs               # 测试
│   └── WEB30Token.sol             # Solidity 版本
└── sdk/                           # TypeScript SDK
    ├── package.json
    ├── tsconfig.json
    ├── src/                       # SDK 源码
    ├── examples/                  # 使用示例
    └── README.md                  # SDK 文档
```

## 1. 编译 Rust 合约

### 安装依赖

```bash
# 安装 Rust (如果未安装)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 添加 WASM 目标
rustup target add wasm32-unknown-unknown
```

### 编译合约

```bash
cd contracts/web30/core

# 调试版本
cargo build

# 发布版本（优化）
cargo build --release

# WASM 版本
cargo build --target wasm32-unknown-unknown --release
```

### 运行测试

```bash
# 所有测试
cargo test

# 详细输出
cargo test -- --nocapture

# 特定测试
cargo test test_token_creation
```

## 2. 部署 Solidity 合约

### 安装 Foundry

```bash
# Windows (PowerShell)
irm get.scoop.sh -outfile 'install.ps1'
.\install.ps1 -RunAsAdmin
scoop install foundry

# Linux/Mac
curl -L https://foundry.paradigm.xyz | bash
foundryup
```

### 编译合约

```bash
cd contracts/web30/core

# 编译
forge build

# 查看输出
ls out/WEB30Token.sol/
```

### 部署到本地测试网

```bash
# 启动本地节点
anvil

# 部署（在另一个终端）
forge create WEB30Token \
  --constructor-args "SuperVM Token" "SVM" 18 1000000000000000000000000 \
  --private-key 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
```

### 部署到公共测试网

```bash
# 设置环境变量
export RPC_URL="https://sepolia.infura.io/v3/YOUR_KEY"
export PRIVATE_KEY="your-private-key"

# 部署
forge create WEB30Token \
  --rpc-url $RPC_URL \
  --private-key $PRIVATE_KEY \
  --constructor-args "SuperVM Token" "SVM" 18 1000000000000000000000000

# 验证合约（Etherscan）
forge verify-contract \
  --chain-id 11155111 \
  --compiler-version v0.8.20 \
  CONTRACT_ADDRESS \
  WEB30Token \
  --constructor-args $(cast abi-encode "constructor(string,string,uint8,uint256)" "SuperVM Token" "SVM" 18 1000000000000000000000000)
```

## 3. 使用 TypeScript SDK

### 安装依赖

```bash
cd contracts/web30/sdk

# 安装
npm install

# 或使用 yarn
yarn install
```

### 编译 SDK

```bash
# 编译 TypeScript
npm run build

# 查看输出
ls dist/
```

### 运行示例

```bash
# 需要先配置环境变量
export RPC_URL="http://localhost:8545"
export TOKEN_ADDRESS="0x..."
export PRIVATE_KEY="0x..."

# 运行转账示例
npx ts-node examples/simple-transfer.ts

# 运行 KYC 示例
npx ts-node examples/kyc-workflow.ts
```

### 在项目中使用

```bash
# 安装到你的项目
npm install ../contracts/web30/sdk

# 或发布到 npm 后
npm install @supervm/web30
```

```typescript
import { SuperVMClient, parseTokenAmount } from '@supervm/web30';

const client = new SuperVMClient({
  rpcUrl: process.env.RPC_URL!,
  privateKey: process.env.PRIVATE_KEY!
});

const token = client.getToken(process.env.TOKEN_ADDRESS!);
await token.transfer('0x...', parseTokenAmount('100', 18));
```

## 4. 测试工作流

### Rust 单元测试

```bash
cd contracts/web30/core

# 运行测试
cargo test

# 覆盖率（需要 tarpaulin）
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

### Solidity 测试

```bash
cd contracts/web30/core

# 创建测试文件 test/WEB30Token.t.sol
forge test

# 带 gas 报告
forge test --gas-report

# 覆盖率
forge coverage
```

### SDK 测试

```bash
cd contracts/web30/sdk

# 运行测试
npm test

# 覆盖率
npm test -- --coverage
```

## 5. 常见问题

### Q: Rust 编译失败
```bash
# 更新工具链
rustup update

# 清理缓存
cargo clean
cargo build
```

### Q: Solidity 编译失败
```bash
# 更新 Foundry
foundryup

# 清理缓存
forge clean
forge build
```

### Q: SDK 类型错误
```bash
# 重新生成类型定义
npm run build

# 清理 node_modules
rm -rf node_modules package-lock.json
npm install
```

## 6. 性能基准测试

### Rust 基准

```bash
cd contracts/web30/core

# 添加 benches/token_bench.rs
cargo bench
```

### Gas 基准（Solidity）

```bash
forge test --gas-report
```

## 7. 安全审计清单

- [ ] 所有 unwrap/expect 已处理
- [ ] 整数溢出检查
- [ ] 重入攻击防护
- [ ] 访问控制验证
- [ ] 事件日志完整
- [ ] 输入验证
- [ ] Gas 优化

## 8. 持续集成

### GitHub Actions 示例

```yaml
name: WEB30 CI

on: [push, pull_request]

jobs:
  rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - run: cd contracts/web30/core && cargo test

  solidity:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: foundry-rs/foundry-toolchain@v1
      - run: cd contracts/web30/core && forge test

  sdk:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
        with:
          node-version: 18
      - run: cd contracts/web30/sdk && npm ci && npm test
```

## 下一步

- 阅读完整 [API 文档](sdk/README.md)
- 查看 [协议标准](../../standards/)
- 参与 [贡献](../../CONTRIBUTING.md)
- 提交 [问题反馈](https://github.com/XujueKing/SuperVM/issues)
