# NOVOVM 统一账户迁移审计快照（SUPERVM vs SVM2026）- 2026-03-06

## 1. 审计目标与范围

本审计回答三个问题：

1. `SUPERVM` 当前统一账户能力进度。
2. `SVM2026` 统一账户相关资产哪些可迁、哪些不可直迁。
3. 在 `SUPERVM` 生产架构下，统一账户迁移应如何切分。

范围：仅审计文档、源码、台账；本次不修改业务代码。

---

## 2. SUPERVM 当前进度（生产基线）

### 2.1 分层能力评估（统一账户专项）

| Layer | Status | 审计结论 | 证据 |
|---|---|---|---|
| Identity Layer（统一主身份） | NotStarted | 未见 `UCA` 主身份实体与生命周期实现 | `crates/novovm-adapter-api/src/ir.rs` |
| Mapping Layer（多链地址映射） | NotStarted | 未见 `UCA <-> PersonaAddress` 绑定索引 | `crates/novovm-adapter-novovm/src/lib.rs` |
| Policy Layer（授权/撤销/会话密钥） | NotStarted | 未见统一权限策略与策略对象 | 当前代码与文档未定义 |
| Nonce/Replay Layer（统一重放防护） | NotStarted | 仅有基础 nonce 字段，不含统一 replay 规则 | `ChainAdapter::get_nonce`、`TxType::Transfer` 演示路径 |
| Audit/Event Layer（账户审计事件） | NotStarted | 未见账户域事件集合与证据出口规范 | 当前代码与文档未定义 |
| Base Account State（基础账户状态） | Ready | 已有 `AccountState{balance,nonce,...}` 基础能力 | `crates/novovm-adapter-api/src/ir.rs` |

结论：`SUPERVM` 统一账户能力目前是“基础状态已就绪，统一账户系统未落地”。

### 2.2 关键边界结论

1. 统一账户系统属于 `SUPERVM` 核心能力域，不属于单链插件域。
2. EVM/BTC/Solana 等插件只能消费账户映射结果，不拥有统一身份主导权。
3. 统一账户应作为 `WP-10` 前置任务，先于 EVM Persona 实施。

### 2.3 全链唯一账户闭环审计声明

当前未见以下闭环定义落地：

- 全链唯一主身份约束（`1 PersonaAddress -> 1 UCA`）
- 冲突绑定拒绝与恢复策略
- 撤销/重绑/冷却期规则
- 全量账户审计事件规范

结论：全链唯一账户仍处于“规范设计前置阶段”，尚未进入实现闭环。

---

## 3. SVM2026 统一账户资产审计（历史实验资产）

### 3.1 可迁移设计资产

- 双标识账户思想：公钥标识 + 数字账户。
- 统一账户实体：含主身份、alias、多链地址关联。
- 多链地址绑定接口：`link/unlink/get_linked_address`。
- 数字账户分配器：号段分配模型。
- 跨链流程中的账户引用模式：交换/合约/挖矿路径均以统一账户为入口。

### 3.2 不可直迁项（必须重构）

- `atomic_swap/cross_contract/cross_mining` 多处 TODO/占位逻辑。
- 签名校验、nonce 防重放、tx hash 记录未形成生产闭环。
- 路径与文档存在偏差，存在迁移误导风险。
- 账户标识口径存在一致性风险（20 字节与示例 32 字节混用）。

结论：`SVM2026` 适合作为“模型与规则来源”，不适合“代码平移来源”。

---

## 4. 迁移判断（SVM2026 -> SUPERVM）

| 主题 | 判断 |
|---|---|
| 迁移方式 | 迁模型，不迁实验实现 |
| 实施顺序 | 账户先行，再接 EVM/WEB30 路由 |
| 协议边界 | `eth_*` 保持单链语义；跨链原子由 `web30_*` 协调层承担 |
| 核心责任 | 统一账户归属 SUPERVM Core；插件层仅做链语义适配 |

---

## 5. 开发前置门槛（进入实现前）

1. 发布并冻结 `Unified Account Spec v1`。
2. 发布并冻结 `Unified Account Gate Matrix v1`。
3. 与 EVM 文档对齐：Type 4（7702）支持/拒绝/降级策略明确。
4. 台账具备可执行字段：`Owner/Blocking Dependency/Deliverable`。
