# 修复 TypeScript 错误指南

## 当前错误状态

看到的红色错误是正常的，因为 `node_modules` 尚未安装。

## 修复步骤

### 1. 安装 Node.js（如果还没有）

访问 https://nodejs.org/ 下载并安装 LTS 版本。

### 2. 安装依赖

```bash
cd contracts/web30/sdk
npm install
```

这将安装：
- `ethers@^6.9.0` - 以太坊库
- `@noble/curves@^1.3.0` - 椭圆曲线加密
- `@noble/hashes@^1.3.3` - 哈希函数
- TypeScript 和开发工具

### 3. 验证安装

```bash
# 编译 TypeScript
npm run build

# 应该生成 dist/ 目录
ls dist/
```

## 当前错误解释

### ❌ `找不到模块"ethers"`

**原因**: `node_modules/ethers` 还未安装

**解决**: 运行 `npm install` 后自动修复

### ❌ 示例文件中的导入错误

**原因**: SDK 尚未编译

**解决**: 运行 `npm run build` 生成类型定义

## 快速验证清单

安装完成后，所有红色错误应该消失：

- [x] `tsconfig.json` 已更新（添加 DOM 库）
- [x] `package.json` 依赖已配置
- [ ] 运行 `npm install`（需要手动执行）
- [ ] 运行 `npm run build`（需要手动执行）

## 如果没有 Node.js 环境

SDK 目前仅提供源码参考。可以：

1. **只查看源码**: 所有 `.ts` 文件都是完整的实现
2. **稍后安装**: 等有 Node.js 环境时再编译
3. **使用 Rust 版本**: `contracts/web30/core/` 中的 Rust 实现已可编译运行

## 验证 Rust 代码（已可用）

```bash
cd contracts/web30/core
cargo build
cargo test  # ✅ 已通过
```

Rust 部分无任何错误，可立即使用！
