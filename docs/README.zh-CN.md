# AOEM（Abstract Orchestrated Execution Model）

> 状态说明（2026-04-17）：
> 本文是 AOEM 技术说明文档，不是 NOVOVM/SUPERVM 当前生产运维入口。
> 当前生产入口与主线守门文档请先看：
> - `README.md`
> - `docs_CN/CURRENT-AUTHORITATIVE-ENTRYPOINT-2026-04-17.md`
> - `docs_CN/NOVOVM-NETWORK/NOVOVM-EVM-NIGHTLY-SOAK-SOP-2026-04-17.md`

**第一个不依赖执行顺序定义正确性、具有量子计算同构性的代数化通用并行执行引擎**

_把并发从调度问题变成语义原语 · 具有量子计算同构性 · 让未来硬件（GPU/量子/ZKVM）参与可信执行_

---

## 什么是 AOEM

AOEM（Abstract Orchestrated Execution Model，抽象编排执行模型）是一个**代数化语义驱动**的通用并行执行引擎。它不是为某一特定领域（如区块链）设计，而是从根本上重新定义了"并发正确性"的数学基础，使得 CPU、GPU、ZKVM 乃至未来的量子计算单元能够在**同一语义框架下**协同工作。

## 产品定位契约（宿主覆盖）

AOEM 是通用执行内核与二进制能力层，不是单一链节点产品。

- AOEM 负责：执行语义、执行路由、能力契约、稳定 FFI。
- 宿主负责：领域协议、网络/共识、治理与产品逻辑。
- 典型宿主：区块链、AI、OS、机器人及其他分布式系统。
- 原则：宿主调用 AOEM，不在宿主重做 AOEM 语义。

### 核心洞见

#### 1. 代数化语义：从"顺序轨迹"到"等式理论"

传统系统通过**"唯一执行顺序"定义正确性**（时间戳、锁、MVCC 等），这根本性地导致：
- GPU 天然并行性无法安全接入系统
- 分布式并发需要全局协调（时钟同步/版本号）
- 验证必须跟踪完整执行轨迹（proof size 不可控）

**AOEM 采用代数化语义**：
- 正确性定义为**"等式理论下的可观察量不变"**
- 执行项可通过**等式重写**进行语义保持的重排
- 调度器不是"时序协调器"，而是**"语义保持的重写引擎"**

```
传统：t₁ → t₂ → t₃  （必须按此时间顺序执行）
AOEM：t₁ ‖ t₂ · t₃   （若 independent(t₁, t₂)，可交换/并行）
```

**这不是"优化"，而是语义级别的范式转变**：

> 并发不再是"调度问题"，而是**程序本身的语义原语**。

#### 2. 与量子计算的数学同构性（核心突破）

AOEM 的代数化语义与量子计算在**形式结构上高度同构**：

| 概念          | AOEM（经典代数叠加）                      | 量子计算（物理叠加）                           |
|-------------|-----------------------------------|-----------------------------------------|
| **状态空间**  | 抽象语义空间（代数对象）                  | 希尔伯特空间（复数向量）                        |
| **叠加**      | 多路径语义叠加 `a·s₁ ⊕ b·s₂`          | 量子态叠加 `α\|0⟩ + β\|1⟩`               |
| **算子**      | 语义变换算子（rewrite/effect）        | 量子门（酉算子/量子通道）                       |
| **观测**      | commit/finalize/resolve（收敛）   | measurement（投影/采样/坍缩）                |
| **可交换性**  | independent → 可重排              | 量子门可交换 → 可并行门层                       |
| **不确定性**  | 多路径延迟决策（可回滚）              | 测量前叠加态（不可克隆）                        |

**统一执行循环**（同时适用于 AOEM 和量子计算）：
```
σ₀ = init()
for op in program:
    σ = apply(op, σ)        // 经典：状态变换 | 量子：酉演化
    if needs_observe(op):
        (c, σ) = observe(σ)  // 经典：commit   | 量子：measurement
return outputs
```

**关键洞见**：

> **AOEM 的代数化语义，把并发执行的不确定性表示为"可组合的语义叠加"；  
> 量子计算把物理态表示为"可组合的线性叠加"。  
> 两者共享"算子作用 + 合流观测"的结构，因此可在同一执行框架下统一调度与后端插拔。**

这使得 AOEM 成为：

> **"量子计算软件层的祖型（Archetype）"——经典世界里，最像量子计算的软件系统。**

**同一 IR，可插拔后端**（AOEM × Quantum 统一模型）：

同一套 IR/调度框架可同时运行：
- **Classical Backend**：CPU/GPU 经典并发
- **Quantum Simulator**：状态向量/tensor network 模拟
- **Quantum QPU**：对接云量子硬件（IBM/AWS/Azure）

只需切换：
- **状态空间定义**：代数对象 → 希尔伯特空间
- **算子约束**：代数 rewrite → 酉算子
- **观测语义**：commit → measurement

**同构点**：
- ✅ 线性组合：语义叠加 ≈ 量子叠加
- ✅ 延迟决策：commit 延迟 ≈ 测量延迟
- ✅ 算子组合：effect composition ≈ 量子门组合
- ✅ 最终坍缩：validate → 确定路径 ≈ measurement → 经典结果

**关键差异**：
- ❌ AOEM **无物理不确定性**（可复制/可回滚/可枚举）
- ❌ AOEM **无 no-cloning 约束**
- ❌ AOEM **无量子纠缠（物理意义）**
- ✅ AOEM 是**可计算的叠加**（deterministic convergence）

#### 3. 三层执行架构：统一语义，异构后端

```
+-------------------------------------------------------------------+
|                 代数化语义层 (Algebraic Semantics)                 |
| Execution Terms:  . || + Tx                                       |
| Equational Theory: 交换律 | 结合律 | 并发提升 | 事务消解            |
| Normal Form: (R1||...||Rn) . (W1...Wm) . Emit*                    |
+-------------------------------------------------------------------+
                         |
                         v
+------------------+------------------+------------------+----------+
| CPU 后端          | GPU 后端          | ZKVM 后端        | Quantum  |
| (语义锚点)        | (批并行执行)      | (可验证证明)     | Backend  |
| 裁决/回退         | SPIR-V           | 规范形证明        | (未来)   |
|                  |                  |                  |          |
+------------------+------------------+------------------+----------+
```

- **CPU**：提供参考实现、裁决机制、统一接口（strict/lenient 双模式）
- **GPU**：基于 SPIR-V compute IR（非 CUDA，跨厂商），首次实现"GPU 作为系统级可信执行单元"
- **ZKVM**：对**规范形（Normal Form）**进行证明，而非轨迹证明，实现证明可聚合性
- **Quantum**：同一 IR 扩展，经典-量子混合计算（未来工作）

---

## 代数化执行语义（理论基础）

### 形式化定义

**执行项语法**：
```
t ::= atom              // Read(k), Write(k,v), Transfer(a,b,x), Emit(e), ...
    | t · t             // 顺序组合（sequential composition）
    | t ‖ t             // 并行组合（parallel composition）
    | t ⊕ t             // 选择/分支（choice/branching）
    | Tx(t)             // 事务作用域（transactional scope）
```

**核心等式**：
```
顺序结合律:   (t₁ · t₂) · t₃ = t₁ · (t₂ · t₃)
顺序单位元:   Nop · t = t = t · Nop

并行交换律:   t₁ ‖ t₂ = t₂ ‖ t₁              if independent(t₁, t₂)
并行结合律:   (t₁ ‖ t₂) ‖ t₃ = t₁ ‖ (t₂ ‖ t₃)

并发提升:     (t₁ · t₂) ‖ t₃ = (t₁ ‖ t₃) · t₂    if independent(t₂, t₃)

事务消解:     Tx(t · Fail · u) = Nop
失败吸收:     Fail · t = Fail = t · Fail
```

**独立性判定**：
```
independent(t₁, t₂) ⇔ RW(t₁) ∩ RW(t₂) = ∅
```
其中 `RW(t) = (ReadSet(t), WriteSet(t))`

**规范形定理**（Normalization Theorem）：
> 任意执行项 `t` 都可通过等式重写为规范形 `NF(t)`：  
> ```
> NF(t) = Tx( (R₁ ‖ R₂ ‖ ... ‖ Rₙ) · (W₁ · W₂ · ... · Wₘ) · Emit* )
> ```
> 且满足：`t ≡ NF(t)`（语义等价）

**这是 GPU 批执行、ZK 证明聚合、量子电路编译的数学基础。**

### 语义等价与调度正确性

**定义（语义等价）**：
```
t₁ ≡ t₂  ⇔  ∀M, O(⟦t₁⟧ₘ) = O(⟦t₂⟧ₘ)
```
其中：
- `M` 为满足等式理论的模型
- `O` 为观察函数（状态根、余额、事件流等可观察量）

**调度正确性定理**（Scheduler Correctness）：
> 若调度器仅使用等式理论中的重写规则，  
> 则其生成的任何执行计划，均与原始程序语义等价。

这意味着：

> **AOEM 的调度层是一个语义保持的重写系统（sound rewriting system）**

### 为什么这很重要（范式对比）

| 维度           | 传统系统                              | AOEM                              |
|--------------|-----------------------------------|---------------------------------|
| **并发定义**   | 调度问题（OCC/锁/时间戳/MVCC）          | 语义原语（代数组合 `‖`）               |
| **GPU 角色**  | 外设加速器（untrusted）               | 一等执行单元（trusted backend）       |
| **正确性锚点** | 全局顺序一致性（total order）          | 可观察量在等式理论下不变（观测语义）       |
| **ZK 证明**   | 证明执行轨迹（trace proof）            | 证明规范形（normal form proof，可聚合）|
| **可组合性**   | 全局状态机（组合时状态空间爆炸）         | 模块化代数效应（effect composition）   |
| **调度器**     | 全局协调器（需要时钟/版本/锁管理）        | 重写引擎（局部等式变换）                |

**AOEM 的真正壁垒不是实现复杂度，而是认知门槛**：

> 敢于放弃"唯一执行顺序"作为正确性的唯一锚点。

---

## GPU 执行引擎（AOEM-GPU）

### 为什么 GPU 以前无法成为系统级执行单元

传统系统要求**全局执行顺序**（时间戳/锁/MVCC）作为正确性锚点，而 GPU 天生不服从顺序：
- ❌ CUDA 执行**非确定性**（warp scheduling 不可控）
- ❌ 结果**不可证明**（无法作为共识/法律级输入）
- ❌ 运行时**不可裁决**（没有"真值源"）
- ❌ 跨厂商**无统一语义**（CUDA 生态锁定）

**AOEM 的突破**：

> **我们的语义不再要求顺序，只要求等价。**  
> GPU 做的只是"在等价类里选一个执行代表"。

这一步，是 GPU 被语义正式接纳的时刻。

### 能力分层

#### 1) AFP（Atomic Function Primitives）层
**定位**：通用系统级 GPU 并行原语，提供执行骨架

**核心原语**：
- Scan / Prefix-Sum
- Reduce / Partition
- Scatter / Gather
- Histogram / Reorder

**不是"算法库"，而是**：
> **"GPU 在系统中被安全、稳定、可预测使用的最小执行骨架"**

**关键特性**：
- 基于 **SPIR-V compute IR**（非 CUDA，跨厂商）
- **Strict CPU-GPU 一致性验证**（bit-exact）
- **可裁决**（CPU 作为真值源）
- **可复现**（deterministic execution）

**解决的根本问题**：
- GPU 原生并行 ≠ 系统级可组合执行
- CUDA 原语不可验证、不可裁决
- 多 GPU 厂商缺乏统一执行语义

> **AFP 是 AOEM 的地基。没有 AFP，AOEM 只能是"算法集合"，而不是"执行中间层"。**

#### 2) MSM（Multi-Scalar Multiplication）层
**定位**：首个"CUDA 生态结构性无法覆盖"的重量级场景

**能力定义**：
- 10⁵–10⁶ 规模椭圆曲线多标量乘法
- **金融级确定性要求**（zero-tolerance for non-determinism）
- 直接进入 **ZK/共识/状态证明链路**

**为什么 MSM 是关键分水岭**：

CUDA 的问题不在性能，而在：
- ❌ 执行结果不可证明
- ❌ 运行时不可裁决
- ❌ 无法作为共识或法律级输入

AOEM MSM 提供：
- ✅ CPU/GPU 严格一致性（可自动验证）
- ✅ 可审计、可复现执行路径
- ✅ 可进入 ZK/状态证明体系

**工程实证（已冻结论文级结论）**：
- ✅ **真实 bucket-list 分布回放机制**（SPIR-V pipeline 导出 + dataset 回放）
- ✅ **Auto-tuning 策略稳健性验证**：overall_hit_rate = 100%（多 seed/多轮）
- ✅ **热/频漂移量化**：证明大量"性能回退"源于测量噪声而非代码变化

**已冻结的 3 篇论文骨架**（Project Canon 级别）：
1. **GPU MSM Auto-Tuning** - 基于真实分布的数据驱动策略
2. **AOEM-GPU 执行模型** - GPU 作为通用可验证执行单元的系统架构
3. **GPU Benchmark 稳健性方法论** - 热/频漂移下的可复现基准测试

> **MSM 是 AOEM 第一个"CUDA 生态结构性无法覆盖"的重量级能力，证明 AOEM 不是理论中间层，而是现实不可替代执行引擎。**

#### 3) CPU 后端
**定位**：统一执行语义锚点

**承担职责**：
1. **执行语义参考实现**（semantic reference）
2. **GPU 结果的裁决与验证基准**（arbiter / ground truth）
3. **系统级统一执行接口**（adapter pattern）

CPU 不追求极限性能，而是**语义正确性的最终仲裁者**。

---

## 验证与证明（ZKVM 集成）

### 规范形证明（vs 轨迹证明）

传统 ZKVM：
```
证明对象：完整执行轨迹（10万条交易 × N 步状态转移）
证明复杂度：O(交错数 × 轨迹长度)
聚合难度：结构不规律，难以批处理
```

AOEM + ZKVM：
```
证明对象：规范形 NF(t) = (R‖...‖R) · (W·...·W) · Emit*
证明复杂度：O(规范形结构) << O(任意交错)
聚合能力：同类规范形可批量证明
```

**为什么规范形证明更高效**：
1. **结构规律**：所有执行都重写到同一形态（Read || Write · Emit）
2. **可复用**：同一规范形结构反复出现 → 证明电路可复用
3. **可聚合**：不需要证明"为什么这么排"，只需证明"规范形执行正确"

**这就是为什么 Phase 13–15 能一路拆阶段、定位瓶颈、做 bucket-list、做 passthrough —— 数学地基是对的。**

---

## 量子-经典统一执行模型（扩展方向）

### 统一 IR（最小可用版）

```
QInit(q)                    // 量子：制备 |0⟩  | 经典：初始化寄存器
QGate(g, targets, ctrls)    // 量子：酉门     | 经典：并行算子
QMeasure(q -> C[i])         // 量子：测量     | 经典：observe
COp(op, args)               // 经典运算       | 量子：feed-forward 后的经典逻辑
Branch(cond, then, else)    // 经典分支       | 量子：基于测量结果的分支
Barrier(scope)              // 调度屏障       | 等价 AOEM 的阶段边界
Observe(tag)                // 统一观测点     | 经典=commit, 量子=measurement bundle
```

### 扩展路径

**Step 1**：新增 `aoem-backend-quantum-sim` crate（类似 GPU backend）
- 2–8 qubit 状态向量模拟
- 基础量子门：H, X, Z, CNOT, Measure

**Step 2**：在 Adapter 层新增 feature gate
- `feature = "quantum"`
- `AOEM_BACKEND=quantum_sim`

**Step 3**：复用现有 Scheduler/Router
- 把"key-space/冲突域"换成"qubit-space"
- 先做互斥调度，跑通再加可交换优化

**Step 4**：同一 IR 双后端 demo
- ClassicBackend：经典概率模拟（对照组）
- QuantumSimBackend：量子态计算（实验组）
- 比较测量直方图

---

## 学术谱系定位

AOEM 不是"凭空发明"，而是对现有理论的**工程可落地综合（synthesis）**：

| 学术领域              | AOEM 采纳与创新                                                   |
|-------------------|---------------------------------------------------------------|
| **并发语义**          | 采纳**部分序（partial order）语义**，把 `‖` 作为一等公民，而非 interleaving |
| **重写系统**          | 调度器是**语义保持的重写引擎**，规范形直接对应硬件/证明友好形态                       |
| **代数效应**          | 副作用模块化（State/Exception/Writer/Resource），允许**可插拔后端**        |
| **线性化/可串行化**      | **选择性线性化**：只在冲突处引入顺序，独立项用等式交换得到并行                         |
| **证明系统**          | 证明**规范形**而非任意交错，减少电路复杂度，提高聚合规律性                           |
| **量子计算**          | 代数叠加与量子叠加**数学同构**，AOEM 可作为量子软件层的经典祖型                      |

**精确学术定位**：

> AOEM 采用部分序语义（partial-order view of execution），  
> 将并发从"实现细节"提升为"语义原语"，  
> 构建语义保持的重写系统以支持异构后端（CPU/GPU/ZKVM/Quantum）。

**可用于论文/白皮书的严谨表述**：

> **AOEM adopts a partial-order view of execution, turning concurrency from an implementation artifact into a semantic primitive.** The scheduler is a **semantics-preserving rewriting engine** targeting hardware- and proof-friendly normal forms. AOEM's semantics is compatible with **algebraic effects**: the same term admits multiple backends via structure-preserving interpretations.

---

## 应用场景

AOEM 是**通用执行引擎**，适用于任何需要**并发、可验证、异构执行**的场景：

### 1. 区块链/Web3
- ✅ 高性能交易执行（OCCC/OCC/MVCC 三路径自适应）
- ✅ GPU 加速状态证明（MSM/Merkle）
- ✅ ZK-Rollup 批验证聚合（规范形证明）
- ✅ 跨链桥可审计执行
- ✅ 第三代分布式互联网执行内核

### 2. 分布式数据库/存储
- ✅ 代数化并发控制（超越 OCC/MVCC）
- ✅ 跨节点语义一致性（无需全局时钟）
- ✅ 可审计执行路径（合规/金融）

### 3. 科学计算/高性能计算
- ✅ 异构硬件统一调度（CPU/GPU/TPU/FPGA）
- ✅ 可复现计算结果（bit-exact across backends）
- ✅ 中间执行状态可验证（checkpointing + proof）

### 4. 量子-经典混合计算（未来方向）
- ✅ 同一 IR 支持经典与量子后端
- ✅ 量子模拟器快速验证（本地调试）
- ✅ 量子硬件接入（IBM Qiskit/AWS Braket/Azure Quantum）
- ✅ 经典-量子 feed-forward 协同

### 5. AI/机器学习推理
- ✅ 模型执行可验证（trustworthy AI）
- ✅ GPU 推理加速 + CPU 裁决
- ✅ 联邦学习中的可信聚合

---

## 项目结构

```
AOEM/
├── crates/
│   ├── core/              # AOEM 核心：代数语义、执行模型
│   ├── adapter/           # 后端适配器（CPU/GPU/ZKVM）
│   ├── backend/           # CPU 参考实现
│   ├── storage-backend/   # 存储后端接口
│   ├── runtime/           # 执行运行时
│   ├── ffi/               # 外部接口
│   ├── optional/          # 可选组件
│   │   ├── zkvm-executor/ # ZKVM 集成
│   └── tests/             # 集成测试
├── docs/
│   └── whitepaper/
│       ├── capability-layer-diagram.md
│       ├── project-canon-aoem-gpu-research-skeletons.md
│       └── algebraic-semantics/
│           ├── semantics.md
│           ├── 01-algebraic-vs-operational-semantics.md
│           ├── 02-algebraic-signature-and-key-equations.md
│           ├── 03-semantics-vs-occ-mvcc.md
│           ├── 04-aoem-execution-gpu-zk-3d-mapping.md
│           ├── 05-why-gpus-failed-before.md
│           ├── 06-formal-definition-paper-level.md
│           └── 07-academic-lineage-positioning.md
├── docs-CN/
│   ├── README.zh-CN.md
│   └── 技术白皮书/
│       ├── AOEM(中文).pdf
│       ├── AOEM+GPU分布式可验证通用 GPU 并行执行引擎(独立技术白皮书).pdf
│       ├── AOEM(CPU-GPU)执行语义映射表.pdf
│       ├── AOEM三层执行架构×GPU-映射文字图.jpg
│       ├── AOEM能力分层图.md
│       ├── project_canon_aoem_gpu_research_skeletons.md
│       └── AOEM代数语义/
│           ├── SEMANTICS.md                     # 代数语义规范
│           ├── 1-代数语义和原语义的区别.md
│           ├── 2-AOEM交易语义的代数签名和关键等式.md
│           ├── 3-语义完整对标OCC和MVCC.md
│           ├── 4-AOEM×执行层×GPU×ZK的三维映射.md
│           ├── 5-Why_GPUs_failed_before.md
│           ├── 6-AOEM代数化语义的形式化定(论文级).md
│           └── 7-AOEM 在学术谱系里的位置.md
├── examples/              # 示例代码
├── scripts/               # 构建/测试脚本
├── vendor/                # 依赖（离线构建）
└── Cargo.toml
```

---

## 快速开始

### 前置要求
- Rust 1.93+ （推荐使用 nightly）
- C++17 编译器（Windows: MSVC 14.44 / Visual Studio 2022）
- CMake 3.31+
- **RocksDB 依赖**（已 vendor 化）

### 构建

```powershell
# 标准构建
cargo build --release

# 启用 GPU 后端（需要 Vulkan）
cargo build --release --features gpu

# 启用 ZKVM 集成
cargo build --release --features zkvm

# 完整构建（所有后端）
cargo build --release --all-features
```

### 运行测试

```powershell
# 单元测试
cargo test

# 集成测试
cargo test --test '*' --release

# Benchmark（需要 nightly）
cargo +nightly bench
```

### 开发模式

```powershell
# 使用 cargo-aoem 脚本（推荐）
.\scripts\cargo-aoem.ps1 build
.\scripts\cargo-aoem.ps1 test
.\scripts\cargo-aoem.ps1 bench

# 验证 RocksDB 集成
.\scripts\Validate-RocksDB-Integration.ps1
```

---

## 技术文档

### 核心概念
- [代数语义规范](ZH-CN/SEMANTICS.md) - 形式化定义与等式理论
- [能力分层图](ZH-CN/AOEM能力分层图.md) - 从执行骨架到不可替代算子
- [AOEM × GPU 技术白皮书](ZH-CN/AOEM+GPU分布式可验证通用%20GPU%20并行执行引擎(独立技术白皮书).pdf)

### 学术深度
- [代数语义和原语义的区别](ZH-CN/1-代数语义和原语义的区别.md)
- [AOEM 在学术谱系里的位置](ZH-CN/7-AOEM%20在学术谱系里的位置.md)
- [Why GPUs Failed Before](ZH-CN/5-Why_GPUs_failed_before.md)

### 工程实践
- [Project Canon - 已冻结论文骨架](ZH-CN/project_canon_aoem_gpu_research_skeletons.md)
- [AOEM×执行层×GPU×ZK 三维映射](ZH-CN/4-AOEM×执行层×GPU×ZK的三维映射.md)

---

## 关键引用（准备投稿的论文）

### Paper 1: GPU MSM Auto-Tuning（可独立投稿）
**标题**：A Data-Driven Auto-Tuning Strategy for GPU MSM under Realistic Bucket Distributions

**核心贡献**：
- 构建真实 bucket-list 分布的 GPU replay 机制
- 发现并量化 auto 策略在真实分布下的灾难性误判
- 提出保守型 auto-local-size 策略
- 多 seed/多轮稳健验证：overall_hit_rate = 100%

### Paper 2: AOEM-GPU 执行模型（战略级主线论文）
**标题**：AOEM-GPU: A General-Purpose Verifiable Execution Model for GPU-Based Distributed Systems

**核心贡献**：
- 提出 AOEM-GPU 执行模型（GPU 作为通用执行引擎）
- 使用 SPIR-V passthrough 作为执行 IR（非 CUDA）
- correctness-first 的 GPU 执行路径
- 提炼 GPU 优化的"结构刀"方法论

### Paper 3: GPU Benchmark 稳健性方法论
**标题**：Robust Benchmarking of GPU Kernels under Thermal and Frequency Drift

**核心贡献**：
- 系统性量化 GPU 热/频漂移对 microbench 的影响（5–40% 波动）
- 提出稳健 benchmarking 方法：A/B 交替 + median-of-medians
- 证明大量性能回退源于测量噪声而非代码变化

---

## 贡献指南

AOEM 目前处于研发阶段，暂不接受外部贡献。项目开源后将提供详细的贡献指南。

---

## 许可证

本项目采用双许可证模式：
- **商业许可证**（LICENSE.COMMERCIAL）- 商业使用需联系授权
- **开源研究许可证**（LICENSE.RESEARCH）- 学术研究与非商业用途

第三方依赖许可详见 [THIRD_PARTY_NOTICES.txt](../THIRD_PARTY_NOTICES.txt)

---

## 联系方式

- **技术咨询**：leadbrand@me.com
- **商务合作**：leadbrand@me.com
- **学术交流**：leadbrand@me.com

---

## 致谢

AOEM 的理论基础源自多个领域的研究积累：
- 并发语义理论（Pomset semantics, Event structures）
- 代数效应与 Handler 理论（Algebraic effects）
- 形式化验证（Term rewriting systems, Confluence）
- 量子计算理论（Linear superposition, Quantum gates）

感谢所有为这些领域做出贡献的研究者。

---

**AOEM 不是更快的执行引擎，而是一个允许未来硬件参与可信执行的语义框架。**
