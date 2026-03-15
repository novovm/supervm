<!--
Copyright (c) 2026 AOEM SYSTEM TECHNOLOGY
All rights reserved.
Author: AOEM SYSTEM TECHNOLOGY
-->

# AOEM 介绍（AOEM Introduction, V1, 2026-03-15）

## 1. AOEM 是什么（What AOEM Is）

AOEM（Abstract Orchestrated Execution Model）是一个**通用执行内核（General-Purpose Execution Kernel）**，不是链专用节点程序。  
它以统一语义与稳定 FFI 对外提供执行与能力路由，宿主（如 SuperVM）在其上实现协议与产品逻辑。

核心定位：
- 执行语义内核（Execution Semantics Core）
- 多后端路由层（Backend Routing Layer）
- 稳定二进制能力接口（Stable FFI Capability Interface）

## 1.1 术语解释（Terminology）

### 可选组件插件（Optional Plugin Component）

你之前看到的“侧车（Sidecar）”就是这个概念。为避免歧义，本文件统一使用“可选组件插件”。

含义：
- 主核心库（core FFI）之外，按需装载的独立能力模块；
- 可开启、可禁用、可替换，但不改变主 ABI；
- 典型能力：持久化、WASM、zkVM、ML-DSA、KMS/HSM 等。

边界：
- 可选组件插件不是“外挂业务逻辑”，而是 AOEM 能力层的一部分；
- 宿主只负责配置与调用，不重写 AOEM 语义。

## 2. AOEM 不是黑盒（Not a Black Box）

AOEM 在 SuperVM 中具备可审计证据链，不是“不可见黑箱”：

1. 头文件契约（Header Contract）  
   - `aoem/include/aoem.h`
2. 能力描述（Capability Manifest）  
   - `aoem/manifest/aoem-manifest.json`
3. 权威能力矩阵（Capability Matrix）  
   - `docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`
4. 权威宿主参数表（Host Call Parameters）  
   - `docs_CN/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`

以上四项可直接用于对外评审和验收复核。

## 3. 产品边界（Product Boundary）

AOEM 负责：
- 执行与并发语义（Execution/Concurrency Semantics）
- 后端选择与回退（Backend Selection/Fallback）
- 能力契约与 FFI 稳定性（Capability Contract/FFI Stability）

SuperVM 负责：
- 领域协议（Domain Protocol）
- 网络与共识（Network/Consensus）
- 治理与业务策略（Governance/Product Logic）

边界规则：**宿主集成 AOEM，不在宿主重复实现 AOEM 执行语义**。

## 4. 能力域总览（Capability Domains）

| 能力域（Capability Domain） | 说明 |
| --- | --- |
| 统一入口（Unified Ingress） | `aoem_global_init`、`aoem_execute_ops_wire_v1` |
| GPU 通用算子（GPU Generic Primitives） | 统一入口 `aoem_execute_primitive_v1`，默认 GPU-first（不可用时回退 CPU） |
| 持久化可选组件插件（Persistence Optional Plugin Component） | RocksDB 等可选后端能力 |
| WASM 可选组件插件（WASM Optional Plugin Component） | Wasmtime 路由能力 |
| zkVM 可选组件插件（zkVM Optional Plugin Component） | Trace/Halo2/RISC0/SP1 路由与执行能力 |
| ML-DSA 可选组件插件（ML-DSA Optional Plugin Component） | ML-DSA keygen/sign/verify/batch |
| 密码学能力（Cryptography） | Ed25519/secp256k1/hash |
| 隐私与证明（Privacy & Proof） | Ring Signature/Groth16/Bulletproof/RingCT |
| KMS/HSM 提供方（KMS/HSM Providers） | `local|plugin|none` 模式 |

### 4.1 zkVM 与 Halo2 边界（zkVM vs Halo2 Boundary）

- zkVM 路径：`Trace/RISC0/SP1`，属于虚拟机语义执行与证明路径。
- Halo2 路径：AOEM 原生电路证明引擎（Native Circuit Proving Engine），不是 zkVM 字节码虚拟机。
- 文档与评审应分开统计两类能力，避免把原生电路误写成 zkVM 子集。

### 4.2 知识产权归属口径（IP Boundary Statement）

- AOEM 原生电路（例如 Halo2 原生电路实现）属于 AOEM 的核心实现资产，应独立作为“原生电路能力”描述。
- 宿主（SuperVM）仅集成并调用 AOEM 能力，不应把 AOEM 原生电路实现描述为宿主自有实现。

## 5. 在 SuperVM 的接入角色（Role in SuperVM）

在 SuperVM 主线中，AOEM 作为 D0 执行能力底座，D1 通过稳定 FFI 接线：

- D0：AOEM Core + 可选组件插件（Optional Plugin Component）能力层
- D1：宿主最薄接线（thin host wiring）

补充（MSM/Transformer）：
- MSM：属于证明/隐私链路的专用后端能力，通过 MSM 后端参数进入，不作为 `primitive_kind` 独立枚举。
- Transformer：当前通过 primitive graph profile 路由，不单独新增 `transformer_*` FFI 符号。

主路径原则：
- 默认静默（silent by default）
- 性能优先（performance first）
- 不把诊断逻辑混入热路径（no diagnostic inflation on hot path）

## 6. 对外评审建议（Review Guidance）

若出现“AOEM 能力不透明/未实现”结论，先按以下顺序复核：

1. `aoem/include/aoem.h`（符号事实）
2. `docs_CN/AOEM-FFI/AOEM-FFI-FULLMAX-CAPABILITY-MATRIX-2026-03-12.md`
3. `docs_CN/AOEM-FFI/AOEM-FFI-HOST-CALL-PARAMS-V1-2026-03-10.md`
4. `aoem/manifest/aoem-manifest.json`

这四项一致时，可判定 AOEM 在 SuperVM 中为“可见、可审计、可复核”的集成能力层。

## 7. SuperVM 侧 ZK 专项说明

若评审重点是 SuperVM 的 Halo2/zkVM/Groth16/Bulletproof/RingCT 接入状态，请直接阅读：

- `docs_CN/AOEM-FFI/SUPERVM-ZK-PROOF-INTEGRATION-V1-2026-03-15.md`
