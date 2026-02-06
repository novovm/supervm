# AOEM 专有化许可证方案分析

**日期**: 2026-02-06  
**目的**: 分析如何在保持 AOEM 核心架构隔离的前提下，将其作为专有引擎（类似 washtime）的可行性  
**状态**: ✅ 临时分析文档（后期可删除）

---

## 1. AOEM 在 SuperVM 中的架构地位

### 1.1 核心定位

AOEM（**A**lgebraic **O**ptimistic **E**xecution **M**odel）是 SuperVM 的**执行内核**，而非可选插件：

| 维度 | 说明 |
|------|------|
| **位置** | `aoem/crates/` （monorepo 域） |
| **角色** | 内核级资产（AOEM-CORE），包含并发控制、调度、运行时、后端 |
| **不可替换性** | 必须集成到 SuperVM，无可替代的替代品 |
| **性能关键** | 直接影响 TPS、并发度、内存占用 |

### 1.2 不可替换的子模块

```
AOEM Kernel (不可替换)
├── aoem-core              ← OCC/MVCC/OCCC 并发语义（锁定）
├── aoem-engine            ← 唯一对外的执行入口（稳定 API 表面）
├── aoem-runtime-api       ← WASM 运行时抽象
├── aoem-runtime-wasmtime  ← 具体 WASM 运行时实现
├── aoem-backend-cpu       ← CPU 执行后端
└── aoem-backend-gpu       ← GPU 执行后端（SPIR-V + Vulkan）
```

### 1.3 架构红线（来自 AOEM-AUTHORITATIVE-DESIGN）

> **All kernel assets (execution/compute/runtime/backends/adapters) must live under `aoem/`.**

这意味着：
- ✅ AOEM 代码已实现功能隔离（可分离）
- ✅ 依赖方向清晰（host → stable API only）
- ✅ 设计上支持专有化（已考虑了分离）

---

## 2. 当前许可证状态

### 2.1 全量 GPL-3.0-or-later

```toml
# SVM2026/Cargo.toml [workspace.package]
license = "GPL-3.0-or-later"
```

**继承关系**：
```
workspace.package.license = "GPL-3.0-or-later"
    ↓
aoem-engine/Cargo.toml:       license.workspace = true
aoem-core/Cargo.toml:         license.workspace = true
aoem-backend-gpu/Cargo.toml:  license.workspace = true
（所有 AOEM crates 都继承此许可证）
```

### 2.2 GPL-3.0-or-later 的衍生作品条款

```
若你使用 GPL-3.0 代码：

❌ 你不能私有化衍生作品
   → 任何修改/扩展都必须开源

❌ 你不能将其集成到闭源产品
   → 除非采用双许可或拿到明确授权

✅ 你可以商业化（但必须开源修改）
```

---

## 3. 「像 washtime 一样」作为引擎可行吗？

### 3.1 答案：✅ 可行，但需要双许可证

根据 AOEM 的架构设计，**完全支持依赖方向隔离**：

```
┌─────────────────────────────────────────────────┐
│  SuperVM 应用层（公开 GPL-3.0）                  │
│  ├─ 共识/网络（可私有化）                        │
│  └─ ChainLinker 适配器接口                       │
├─────────────────────────────────────────────────┤
│  AOEM Engine 稳定 API（types 层）              │
│  ├─ ExecPlan, ExecResult, Effects              │
│  └─ BackendPolicy, Decision, EnginePolicy      │
├─────────────────────────────────────────────────┤
│  AOEM 核心实现（可专有化）                      │
│  ├─ aoem-core（OCCC 语义）                    │
│  ├─ aoem-backend（CPU/GPU）                   │
│  └─ aoem-runtime（WASM 运行）                 │
└─────────────────────────────────────────────────┘

当前：GPL-3.0-or-later（全开源）
可选：切换为双许可（开源+商业）
```

### 3.2 技术上的隔离保证

AOEM 已设计为**最小稳定 API 表面**：

```rust
// aoem-types（公开 + 可开源）
pub struct ExecPlan { ... }
pub struct ExecResult { ... }
pub struct Effects { ... }
pub enum BackendKind { Cpu, GpuVulkan, GpuCuda }
pub struct Decision { backend_kind, reason }

// aoem-engine（只导出 trait）
pub trait AoemEngine {
    fn execute_plan_with_policy(
        &self,
        plan: ExecPlan,
        policy: EnginePolicy,
    ) -> Result<EngineOutcome>;
}
```

**SuperVM host 只需依赖这两个 crate**，内部实现完全隔离。

### 3.3 依赖方向隔离（红线）

```
SuperVM host
    ↓ 依赖
aoem-engine + aoem-types （稳定 API）
    ↓ 内部依赖（不对外暴露）
aoem-core, aoem-backend, aoem-runtime
    ↑ 内部代码，可以：
    ├─ 编译为静态库（.a/.lib）
    ├─ 编译为动态库（.so/.dll）
    ├─ 闭源发布
    └─ 单独许可证
```

**关键约束**（来自 AOEM 设计）：
- ✅ `aoem-core` 不依赖 `aoem-runtime` 实现
- ✅ `aoem-backend-gpu` 不依赖 `aoem-runtime`
- ✅ `aoem-engine` 是唯一对外入口
- ✅ Host 只通过稳定 API 调用

---

## 4. 商业可行性对比

| 方案 | 当前（SVM2026） | 专有化双许可 | 完全闭源二进制 |
|------|-----------------|----------|-------------|
| **许可证** | GPL-3.0-or-later | AGPL-3.0 (host) + Commercial (AOEM) | Commercial only |
| **开源代码** | 100% AOEM + SuperVM | Host + API wrapper | 仅提供 headers |
| **IP 保护** | ❌ 衍生作品需开源 | ✅ AOEM 完全闭源 | ✅ 完全闭源 |
| **分发形式** | GitHub source | 混合（源码 + 二进制库） | 二进制 + 许可证 |
| **法律成本** | 低（GPL 明确） | 中（双许可声明） | 高（闭源协议） |
| **开发成本** | 无（已隔离） | 低（仅改 Cargo.toml） | 中（FFI bridge） |
| **社区友好度** | 高 | 中高 | 中 |
| **企业采购** | 中（开源无成本） | 高（可商业化） | 最高（纯商业） |

---

## 5. 具体实施方案

### 方案 A：双许可证（推荐，改动最小）

**目标**：保持代码结构不变，仅改许可证声明

#### 步骤 1：创建商业许可证文件

```bash
SuperVM/
├── LICENSE.GPL-3.0           # 原公开许可证
├── LICENSE.COMMERCIAL        # 新增：商业许可证（仅给付费用户）
├── LICENSE.AOEM-DUAL         # 可选：双许可证声明
└── README.md                 # 更新说明
```

#### 步骤 2：修改 Cargo.toml 许可证标记

```toml
# 对于 SuperVM host/apps （保持开源）
# src/vm-runtime/Cargo.toml
[package]
license = "AGPL-3.0-or-later"  # 或 "GPL-3.0-or-later"

# 对于 AOEM 核心 crates （专有化）
# aoem/crates/core/aoem-engine/Cargo.toml
[package]
license = "LicenseRef-AOEM-Commercial"

# aoem/crates/core/aoem-core/Cargo.toml
license = "LicenseRef-AOEM-Commercial"

# aoem/crates/backend/aoem-backend-gpu/Cargo.toml
license = "LicenseRef-AOEM-Commercial"
```

#### 步骤 3：添加 SPDX 许可证声明

在项目根目录 Cargo.toml 添加：

```toml
[package]
# ...
license = "AGPL-3.0-or-later OR LicenseRef-AOEM-Commercial"

[package.metadata.license]
aoem-commercial = { file = "LICENSE.COMMERCIAL", description = "AOEM Execution Kernel - Commercial License" }
```

#### 步骤 4：源文件头部注释（可选）

```rust
// aoem/crates/core/aoem-engine/src/lib.rs
//
// SPDX-License-Identifier: LicenseRef-AOEM-Commercial OR AGPL-3.0-or-later
//
// This file is part of the AOEM Execution Kernel.
// For licensing information, see LICENSE.COMMERCIAL or LICENSE.GPL-3.0
```

**变更成本**：
- ✅ 无需改代码
- ✅ 仅改 Cargo.toml 和许可证文件（~5 个文件）
- ✅ 依赖方向不变
- ✅ 编译产物相同（source distribution vs binary distribution）

---

### 方案 B：Binary + Wrapper（最严格，改动中等）

**目标**：将 AOEM 编译为二进制库，只发布 API wrapper

#### 步骤 1：编译 AOEM 为静态库

```bash
# build.sh
cargo build --release \
  --package aoem-engine \
  --package aoem-core \
  --package aoem-backend-cpu \
  --package aoem-backend-gpu \
  --target-dir target/aoem-release

# 生成产物
cp target/aoem-release/deps/libaoem_engine.a aoem-bin/
cp target/aoem-release/deps/libaoem_core.a aoem-bin/
cp target/aoem-release/deps/libaoem_backend_cpu.a aoem-bin/
cp target/aoem-release/deps/libaoem_backend_gpu.a aoem-bin/
```

#### 步骤 2：创建 FFI wrapper crate

```bash
SuperVM/
├── aoem-bindings/            # 新增：FFI wrapper（GPL-3.0）
│   ├── Cargo.toml
│   ├── build.rs              # linkage script
│   ├── src/
│   │   ├── lib.rs
│   │   └── ffi.rs            # C ABI definitions
│   └── aoem-headers/          # C headers（从 aoem 导出）
│       ├── aoem_engine.h
│       └── aoem_types.h
└── LICENSE.COMMERCIAL        # AOEM 商业许可
```

**build.rs 示例**：

```rust
fn main() {
    // Link to pre-compiled AOEM libraries
    println!("cargo:rustc-link-search=../aoem-bin");
    println!("cargo:rustc-link-lib=aoem_engine");
    println!("cargo:rustc-link-lib=aoem_core");
    println!("cargo:rustc-link-lib=aoem_backend_cpu");
    
    #[cfg(target_os = "windows")]
    println!("cargo:rustc-link-lib=aoem_backend_gpu");
}
```

**变更成本**：
- ⚠️ 需要 FFI bridge（~200-300 行 Rust）
- ⚠️ 需要 C header 定义（~100 行 C）
- ⚠️ 需要构建脚本（build.rs）
- ✅ 完全隐藏 AOEM 源码
- ✅ 可以单独分发 `.so` / `.dll` / `.a`

---

## 6. 不打破的保证（架构红线）

即使专有化 AOEM，以下保证**不变**：

```
✅ SuperVM host 依然通过稳定 API 调用 AOEM
   → host 代码不变
   → 依赖方向不变
   → 可以保持开源

✅ AOEM 不拥有共识/网络
   → 共识层可以私有化或保持开源（独立选择）
   → 网络层独立可管理

✅ GPU 后端隔离
   → 可以独立许可 GPU 驱动（CUDA/SPIR-V/Vulkan）
   → 每个后端可独立商业化

✅ Plugin 层仍然可开源
   → EVM/Bitcoin/Solana 链接器独立许可
   → ChainLinker trait 保持稳定

✅ 衍生功能可开源
   → ZKVM 层（L2）可开源
   → 应用适配器（adapters）可开源
   → 只有 AOEM-CORE 专有化
```

---

## 7. 最终建议

### 7.1 短期（立即可做）

**采用方案 A（双许可证）**

```bash
1. 创建 LICENSE.COMMERCIAL 文件
2. 修改 aoem/** crates 的 Cargo.toml
   license = "LicenseRef-AOEM-Commercial"
3. 更新 README 说明许可政策
4. Git commit + push

代码零改动，仅改配置文件 5-10 个
开发成本：2-4 小时
法律成本：律师审阅许可证条款（可选）
```

### 7.2 中期（如需更严格保护）

**升级到方案 B（二进制库）**

```bash
1. 编写 build.rs 和 FFI wrapper
2. 预编译 AOEM 为 .a / .so / .dll
3. 创建 aoem-bindings crate
4. 修改 SuperVM 依赖关系

代码改动：~400 行 Rust FFI + 100 行 C headers
开发成本：1-2 周
额外成本：维护多个平台的预编译库
```

### 7.3 长期架构建议

```
保留的开源部分：
├── SuperVM host（AGPL-3.0）
├── 共识层（可选开源）
├── 网络层（可选开源）
├── 应用层（可选开源）
└── ChainLinker plugins（社区维护）

专有化部分：
└── AOEM 内核（商业许可）
    ├── aoem-core
    ├── aoem-engine
    ├── aoem-backend
    └── aoem-runtime
```

**优点**：
- ✅ AOEM 作为核心 IP，单独商业化
- ✅ SuperVM 应用层保持开源，吸引社区
- ✅ 双收入流：开源许可 + 商业 AOEM 授权
- ✅ 架构已支持，改动最小

---

## 8. 关键 Q&A

### Q1：专有化 AOEM 会影响 SuperVM 的开源承诺吗？

**A**: 不会。SuperVM **主体保持开源**（host + consensus + network），只有执行内核（AOEM）专有化。这类似于：
- MongoDB（AGPL-3.0）+ 专有企业特性
- Elasticsearch（Elastic License）+ 开源插件
- Docker（Apache 2.0）+ 专有企业功能

### Q2：用户能否 fork 和修改 SuperVM？

**A**: 完全可以。
- ✅ Fork SuperVM（AGPL-3.0）
- ✅ 修改 host/consensus/network
- ✅ 需要用 AOEM 的二进制库（即依赖 AOEM 商业许可）
- ❌ 不能修改 AOEM 源码（闭源）

### Q3：会影响性能吗？

**A**: 不会。
- 二进制编译后，产物完全相同
- 依赖方向不变，ABI 不变
- 运行时性能、TPS、延迟零差异

### Q4：上游贡献者怎么处理？

**A**: 
- **开源部分**（host）：贡献者保持开源许可，继续享受 GPL
- **AOEM 部分**：需要明确的贡献者协议（CLA），贡献源码时自动转让给 SuperVM 团队进行商业授权
- **建议**：采用 CLA，明确标注哪些 PR 是为了改进 AOEM（商业）vs host（开源）

---

## 9. 法律审查清单

在实施前，建议：

- [ ] 咨询开源许可证律师，确认双许可方案合法性
- [ ] 准备 AOEM 商业许可证条款（参考 Elastic License 或 BSL）
- [ ] 编写贡献者协议（CLA）
- [ ] 更新 README 和 CONTRIBUTING.md
- [ ] 准备客户支持文档（说明许可政策）
- [ ] 设置许可证验证工具（如 REUSE.software）

---

## 附录：参考案例

### MongoDB（AGPL-3.0 + 企业许可）
```
开源：MongoDB Community Server (AGPL-3.0)
商业：MongoDB Enterprise（闭源，需付费）
```

### Elastic（Elastic License + SSPL）
```
开源：Elasticsearch (AGPL-3.0/Elastic License)
商业：Elasticsearch + 企业特性（Elastic License，需付费）
```

### JetBrains（AGPL-3.0 for IDE + 商业许可）
```
开源：IntelliJ IDEA 社区版（AGPL-3.0）
商业：IntelliJ IDEA 企业版（专有许可，需付费）
```

---

**文档完成**。建议后期在制定具体许可政策时再参考本文。

