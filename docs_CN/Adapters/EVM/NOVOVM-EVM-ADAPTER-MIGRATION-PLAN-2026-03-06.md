# NOVOVM EVM/Adapter 迁移方案与实施步骤（SUPERVM）- 2026-03-06

## 1. 文档目的

本方案用于指导将 `SVM2026` 中已实现的 EVM 与 adapter 能力，按 `SUPERVM` 当前架构迁移落地。

范围聚焦：

- EVM 适配能力（交易/区块/状态/合约执行相关能力）。
- 多链插件能力（通过插件集成 Ethereum/BNB/Polygon/Avalanche 等公链）。
- 迁移实施步骤、门禁验收口径、进度记录方式。

约束：

- 当前阶段只产出文档与方案，不改代码。
- 目标是“在优化后的 SUPERVM 架构下恢复并增强旧项目能力”，不是回退到旧架构。

### 1.1 多链插件族定位（补充）

- 以太坊（EVM）只是 `SUPERVM` 多链插件体系中的一个分支，不是唯一分支。
- 后续将按相同治理框架扩展 `BTC/Solana/...` 等公链插件。
- 当前文档只聚焦 `EVM` 分支，原因是其迁移优先级最高、历史资产最完整。
- 总体原则不变：`入口协议多态` + `内核能力统一` + `SUPERVM First`（语义等价时优先复用 SUPERVM）。

## 2. 当前基线（SUPERVM）

### 2.1 已有能力（可复用）

- 已有统一链适配契约：`novovm-adapter-api`（`ChainType/ChainAdapter/TxIR/BlockIR/StateIR`）。
- 已有双后端框架：`novovm-adapter-novovm`（native）+ `novovm-adapter-sample-plugin`（plugin, C ABI）。
- 已有插件治理能力：ABI 版本检查、能力掩码、registry 白名单、负向门禁、consensus binding。
- 已有矩阵与注册表配置：`config/novovm-adapter-compatibility-matrix.json`、`config/novovm-adapter-plugin-registry.json`。

### 2.2 当前缺口（必须补齐）

- `F-13 Multi-chain plugin capability` 仍为 `NotStarted`。
- 文档已明确 `evm/bnb` 在当前阶段属于 compatibility stubs，专用链语义适配器待后续实现。
- 当前 native/plugin 路径的执行语义仍以 `TxType::Transfer` 为主，不是完整 EVM 语义闭环。
- 功能证据主要是 `adapter_expected_chain=novovm`，缺少 `evm/bnb/polygon/avalanche` 的系统化门禁证据。

## 3. 迁移目标

### 3.1 业务目标

- 让 `SUPERVM` 通过插件化方式获得多链交互能力。
- 在不污染核心链路的前提下，为 EVM 系公链提供可上线的适配路径。

### 3.2 技术目标

- 在现有 `novovm-adapter-api` 契约下落地专用 EVM 适配器能力。
- 构建“可替换、可审计、可回退”的 EVM 插件交付模式。
- 建立与 `NOVOVM-CAPABILITY-MIGRATION-LEDGER` 同风格的迁移台账与门禁闭环。

## 4. 推荐迁移方案（基于 SUPERVM 架构）

方案选择：`Plugin-first + Core-shared`。

- `Plugin-first`：优先走插件交付路径，保持核心执行链路与共识主路径稳定。
- `Core-shared`：抽取共享 EVM 核心能力模块（precompiles/gas/execution/translator），供多链插件复用。
- `Chain-profile`：每条 EVM 系链采用 profile 配置（`chain_id`、`hardfork_schedule`、`enabled_tx_types`、`blob_params`、`precompile_set`、`fee_model`、`finality/reorg_policy`、`rpc_compat_level`、`unsupported_eips`、`persona_mode`）。

### 4.1 EVM 插件镜像模式全景图（中文）

全局原则（避免语义歧义）：

- `内核统一`：底层执行能力优先复用 `SUPERVM`。
- `外观多态`：按入口协议选择对外 Persona，而不是全局固定 EVM。
- `web30_*` 入口：对外保持 WEB30 主链语义。
- `eth_*` 入口：对外保持 EVM 节点语义（本文件重点）。
- `btc_*` 等入口：对外保持对应链语义（在对应 adapter 文档展开）。

全景图（从用户到执行）：

```text
[用户/钱包/dApp/SDK]
        |
        | web30_* / eth_* / btc_*
        v
[SUPERVM 插件网关]
        |
        | 入口路由命中 eth_* 分支（本图仅展开该分支）
        v
[节点镜像层（Node Persona）]
  对外身份：表现为“以太坊/目标公链节点”
        |
        v
[能力路由层（Capability Router）]
   |------------------------------|
   |                              |
   v                              v
[SUPERVM 原生能力快路径]         [EVM 链专用插件路径]
(mempool/执行/状态/索引/限流)    (eth/bnb/polygon/avax profile)
   |                              |
   |                              +--> [共享 EVM Core: translator/execution/precompiles/gas]
   |                              +--> [外部链 RPC/P2P]
   |                              |
   |----------------------v-------|
                    [治理与审计层]
        (Registry + ABI/Caps + Hash + Gate + Evidence)
```

分层说明（表格化）：

| 层级 | 中文名称 | 主要职责 | 输入 | 输出/结果 | 默认策略 |
|---|---|---|---|---|---|
| L1 | 用户接入层 | 钱包、dApp、SDK 发起链交互请求 | 用户交易、查询请求 | 多协议请求（`web30_*`/`eth_*`/`btc_*`） | 保持入口协议习惯 |
| L2 | 插件网关层 | 统一鉴权、限流、入口路由（先判定协议/链类型） | 多协议请求 | 已命中目标 Persona 的标准化请求（本文件聚焦 `eth_*`） | 入口即语义 |
| L3 | 节点镜像层（Persona） | 锁定“本请求对外是什么链”的契约语义 | 已命中 EVM Persona 的请求 | 目标链风格响应（字段/错误码/回执） | 优先保证用户感知一致 |
| L4 | 能力路由层 | 按 P0/P1/P2 判定走哪条执行路径 | Persona 请求 | 快路径或插件路径决策 | `SUPERVM First` |
| L5 | SUPERVM 原生能力层 | 复用高性能执行、状态、索引、审计能力 | 路由后的可复用请求 | 高性能结果 | 语义等价即优先复用 |
| L6 | EVM 链插件层 | 处理链专属规则、fork 特性、专属 precompile | 路由后的链专属请求 | 链特定结果 | 仅在语义不等价时启用 |
| L7 | 治理与审计层 | 插件注册、ABI/Caps/Hash 约束、Gate 证据沉淀 | 各路径执行证据 | 可追溯审计与发布门禁信号 | 全路径强制接入 |

说明：

- 本文档是 EVM adapter 专项：仅在 `eth_*` 入口命中时，`L3` 才表现为 EVM 节点语义。
- `web30_*` 入口不应被 EVM Persona 覆盖，仍保持 WEB30 主链语义。
- 该感知由接口与行为保证：兼容 `eth_*` RPC、链 profile、错误码与回执格式。
- 内部执行不要求照搬原链实现；在语义一致前提下优先使用 `L5` 的 SUPERVM 能力。

### 4.1.1 二级路由模型（先后关系）

1. `路由一：入口路由（L2）`  
   按协议与链类型分流：`web30_* -> WEB30 Persona`、`eth_* -> EVM Persona`、`btc_* -> BTC Persona`。
2. `路由二：能力路由（L4）`  
   在 Persona 已锁定后，再按 `P0/P1/P2` 决定内部走 SUPERVM 快路径或链插件路径。
3. `Persona 位于两次路由之间`  
   作用是先固定“对外契约语义”，再优化“内部执行路径”，避免内部优化影响外部链语义稳定性。

### 4.2 镜像模式定义（对用户可见行为）

镜像模式目标：

- 用户侧：钱包、SDK、dApp 连接到 `eth_*` 插件入口时，节点身份与返回语义满足目标 EVM 公链预期。
- 平台侧：内部路由可选择 SUPERVM 快速路径或链专用插件路径，但最终对外行为保持一致。
- 边界侧：`web30_*` 入口继续遵循 WEB30 主链语义，不受 EVM Persona 覆盖。

镜像模式最小交付面（第一阶段）：

- RPC 兼容：`eth_chainId`、`eth_blockNumber`、`eth_getBalance`、`eth_getTransactionReceipt`、`eth_call`、`eth_sendRawTransaction` 等核心接口。
- 数据兼容：交易哈希、区块头关键字段、回执状态、日志 topic 编码、错误码语义。
- 运维兼容：节点健康检查、速率限制、审计日志与可观测指标。

### 4.3 功能重叠时的优先级策略（SUPERVM First）

原则：若 `SUPERVM` 已具备同等语义能力，则优先使用 `SUPERVM` 实现；仅在语义不等价或链规则强绑定时使用链插件实现。

判定分级：

| 等级 | 判定条件 | 默认路由 | 示例 |
|---|---|---|---|
| P0（优先复用） | 与目标链语义等价，且 SUPERVM 性能更优 | SUPERVM Fast Path | tx 入池前置校验、状态读缓存、索引查询、RPC 限流与审计 |
| P1（可复用但需校验） | 大体等价但存在边界差异 | 双跑比对后切 SUPERVM | gas 估算、nonce 策略、receipt 字段映射、日志过滤 |
| P2（链专属） | 目标链强规则绑定，SUPERVM 无等价实现 | EVM Chain Plugin | fork 规则细节、链特定 precompile 行为、reorg/finality 特性 |

执行规则（Router）：

1. 先做能力分类（P0/P1/P2）。
2. P0 直接走 SUPERVM，保留审计证据。
3. P1 先开启 compare gate，状态根/回执一致后切换默认路由。
4. P2 固定走链插件，并受 registry+ABI+hash 治理约束。
5. 任一路径异常时按回退策略降级到安全路径。

### 4.4 EVM Persona 兼容级别定义（替代“节点身份宣称”）

为避免“我们就是完整以太坊客户端”的语义误导，统一采用兼容级别定义：

| 级别 | 名称 | 定义 | 适用阶段 |
|---|---|---|---|
| C0 | RPC Persona Compatible | 对外 `eth_*` RPC 基础行为兼容（读写最小闭环） | M0 |
| C1 | Execution-Compatible Gateway | 执行、回执、日志、错误码语义兼容并具备门禁证据 | M1 |
| C2 | Profile-Hardened Persona | 多链 profile 差异化治理稳定（含 tx type/fork/blob） | M1+ |
| C3 | Advanced Ops Compatible | 含 trace/高级运维兼容能力，仍不等同完整 geth 网络客户端 | M2 |

约束声明：

- 本项目目标是 `RPC/语义兼容`，不是复制 `devp2p + downloader + full sync + mining` 全栈客户端。
- 对外使用“EVM Persona 兼容节点”口径，不使用“完整以太坊节点”口径。

### 4.5 协议边界（WEB30 / EVM / 原子交易）

核心边界：

1. `eth_*` 只承载单链 EVM 语义兼容，不直接承载跨链原子协调语义。
2. 多链原子交易能力归属 `web30_*`/SUPERVM-native 协议层。
3. EVM adapter 在原子交易中作为“链执行适配器”，不是“原子协调器本体”。
4. 若 `eth_*` 请求触发跨链能力，必须通过显式桥接流程并记录语义降级说明。

参考文档：

- `NOVOVM-ATOMIC-ORCHESTRATION-LAYER-SPEC-2026-03-06.md`
- `NOVOVM-WEB30-EVM-SEMANTIC-MAPPING-MATRIX-2026-03-06.md`

### 4.6 账户与签名域边界（全链唯一账户）

强制约束：

1. EVM 地址在 SUPERVM 中必须有明确的统一账户映射关系（可追溯、可治理）。
2. `eth_sign/personal_sign/typed data` 与 `web30` 原生签名域必须隔离定义。
3. `nonce`/权限/授权撤销规则必须以统一账户规范为准，不得在插件内隐式发散。
4. EIP-7702（Type 4）支持策略必须显式写明：`支持/拒绝/降级` 与错误码语义。

参考文档：

- `NOVOVM-UNIFIED-ACCOUNT-AND-EVM-PERSONA-MAPPING-SPEC-2026-03-06.md`
- `NOVOVM-ETHEREUM-PROFILE-2026-COMPAT-BASELINE-2026-03-06.md`

## 5. 迁移工作分解（Work Packages）

| ID | 工作包 | 目标 | 主要产物 | 状态初值 |
|---|---|---|---|---|
| WP-00 | 基线冻结与映射清单 | 锁定当前 SUPERVM 适配器基线与 SVM2026 能力映射 | 能力映射表、差异清单 | Done |
| WP-01 | EVM 核心能力下沉 | 将 SVM2026 EVM 可复用模块迁入 SUPERVM 目标结构 | shared evm core 设计与目录方案 | NotStarted |
| WP-02 | EVM IR 翻译器 | 补齐原始 EVM tx/block 到 `TxIR/BlockIR` 的映射 | translator 设计、字段映射规则 | NotStarted |
| WP-03 | EVM 插件实现 | 在现有 ABI/registry 框架内实现专用 EVM 插件 | plugin crate 设计、ABI 对齐说明 | NotStarted |
| WP-04 | 多链 profile 扩展 | ETH/BNB/Polygon/Avalanche 配置与路由策略 | chain profile 配置规范 | NotStarted |
| WP-05 | 门禁与证据链 | 建立 evm/bnb/polygon/avax 四链正负向门禁 | gate 脚本方案、产物路径规范 | NotStarted |
| WP-06 | 稳态与安全加固 | 插件升级、回滚、hash/ABI 治理、压力稳定性 | 风险与回滚手册 | NotStarted |
| WP-07 | RC 收口 | 形成 ReadyForMerge/SnapshotGreen 口径 | rc-candidate 口径定义 | NotStarted |
| WP-08 | 镜像节点交互面 | 输出 EVM Persona 兼容级别规范（C0~C3） | RPC/回执/错误码兼容规范 | NotStarted |
| WP-09 | 功能重叠路由优化 | 建立 P0/P1/P2 分类并默认 SUPERVM First | 能力重叠矩阵 + 路由决策表 | NotStarted |
| WP-10 | 统一账户映射规范 | 建立全链唯一账户与 EVM Persona 映射规则 | 账户映射规范、签名域/nonce/权限规则 | NotStarted |
| WP-11 | 原子协调层规范 | 明确多链原子交易归属与编排协议 | 原子协调规范、入口边界约束 | NotStarted |
| WP-12 | WEB30↔EVM 映射矩阵 | 明确可映射/部分映射/不可映射能力 | 协议映射矩阵、语义损失清单 | NotStarted |
| WP-13 | Ethereum 2026 兼容基线 | 定义 tx type/fork/blob/7702 的目标兼容面 | EVM profile 基线与分阶段目标 | NotStarted |

## 6. 分阶段实施步骤

### Phase 0：准备阶段（文档与基线）

1. 输出迁移映射：`SVM2026 -> SUPERVM` 的模块映射与语义差异。
2. 固化“不可回退约束”：不改共识主链路、不引入旁路执行、不破坏现有 adapter gate。
3. 输出镜像节点 persona 清单（对外 RPC/数据/错误语义）。
4. 输出统一账户映射规范（地址/签名域/nonce/权限）。
5. 输出 `WEB30↔EVM` 语义映射矩阵与原子交易边界说明。
6. 建立本目录台账文件并初始化状态。

出口标准：

- 迁移目标、范围、验收口径达成一致。
- 协议边界（`eth_*` 与 `web30_*`）和原子协调归属达成一致。
- 账户映射规范进入可评审状态。
- 台账初始化完成，可按日/按里程碑更新。

### Phase 1：EVM 核心迁移设计

1. 明确可迁入模块边界：precompiles、gas、execution、translator。
2. 统一到 `novovm-adapter-api` 的 IR 与错误语义。
3. 完成功能重叠盘点（P0/P1/P2），并形成 SUPERVM First 初版路由。
4. 完成 `tx type 0/1/2/3/4` 与 profile 基线映射（含 Type 4 策略声明）。
5. 约束执行策略：先兼容后增强，避免一次性大改。

出口标准：

- 形成 `shared evm core` 结构设计文档。
- 形成字段映射与兼容策略表（含 unsupported 行为与错误码策略）。

### Phase 2：插件化落地设计

1. 设计 `novovm-adapter-evm-plugin-*` 的 ABI 适配层。
2. 复用现有 registry/abi/caps/hash 治理机制。
3. 设计 `auto/native/plugin` 路由下的 EVM 专用选择规则。
4. 输出“镜像节点兼容规范 v1”（RPC、回执、错误码、日志、filter、tx type）。

出口标准：

- ABI 与 registry 接入设计完成。
- 版本升级策略（兼容/破坏性）完成。

### Phase 3：多链 profile 与门禁设计

1. 设计 ETH/BNB/Polygon/Avalanche profile（含 fork/tx type/blob/precompile 差异）。
2. 新增链级门禁口径：`adapter_expected_chain=evm/bnb/polygon/avalanche`。
3. 定义负向场景：ABI mismatch、registry mismatch、symbol mismatch、state_root mismatch、unsupported tx type、错误码漂移。
4. 对 P1 能力启用 compare gate，达标后切换到 SUPERVM 默认路径。
5. 完成协议语义 gate（tx type/receipt/error/filter/reorg/account behavior）。

出口标准：

- 每条链至少 1 组正向 + 1 组负向 gate 方案。
- 2026 基线兼容项（含 blob 与 Type 4 策略）完成状态可追踪。
- 证据路径规范与汇总格式完成。

### Phase 4：发布与收口

1. 将 EVM/adapter 迁移状态纳入 capability ledger 风格报告。
2. 形成 RC 验收口径：功能、稳定性、治理、安全四项同时绿色。
3. 输出回退与应急手册。

出口标准：

- `F-13` 从 `NotStarted` 推进到 `ReadyForMerge`（按证据达成）。
- RC 复现路径可重复执行。

## 7. 门禁与验收口径（EVM 专项）

必选信号：

- `adapter_signal`：链/后端/交易规模与基础可用性。
- `adapter_plugin_abi_signal`：ABI 与能力掩码一致性。
- `adapter_plugin_registry_signal`：hash/whitelist/chain_allowed 一致性。
- `adapter_backend_compare_signal`：同输入跨后端状态根一致性。
- `adapter_consensus_binding_signal`：插件绑定哈希一致性。
- `evm_semantics_signal`（新增）：合约调用、部署、事件日志、gas 行为一致性。
- `persona_compat_signal`（新增）：对外节点身份与 RPC/回执语义兼容性。
- `overlap_router_signal`（新增）：P0/P1/P2 路由命中率与一致性统计。
- `evm_tx_type_signal`（新增）：Type 0/1/2/3/4 兼容性与拒绝策略一致性。
- `evm_receipt_log_signal`（新增）：receipt 字段、topic、effectiveGasPrice/blob 字段一致性。
- `evm_error_code_signal`（新增）：nonce/replacement/intrinsic/unsupported 等错误码语义一致性。
- `evm_filter_subscribe_signal`（新增）：`eth_getLogs/newFilter/getFilterChanges/subscribe` 兼容性。
- `evm_reorg_finality_signal`（新增）：reorg 下回执稳定性与最终性策略一致性。
- `evm_account_behavior_signal`（新增）：EOA/contract/delegated/Type4 账户行为一致性。

必选通过条件：

- ETH/BNB/Polygon/Avalanche 四链 profile 全通过。
- 正向/负向 gate 全覆盖并通过。
- adapter stability 通过率达标（建议先 `runs>=3`，再扩展到 `runs>=10`）。
- P0 能力默认走 SUPERVM 且稳定；P1 能力 compare 达标后切换；P2 能力受插件治理约束。
- `eth_*` 与 `web30_*` 边界无污染：跨链原子协调仅在 `web30_*` 生效。
- 统一账户映射规范与 Type 4（7702）策略在门禁中可验证（支持或显式拒绝皆可，但必须一致）。

## 8. 风险与回退策略

主要风险：

- 旧实现中存在“演示版 EVM 引擎/骨架插件”语义，与生产级目标存在差距。
- 迁移后若直接切主路径，可能引入状态根偏差与治理风险。

控制策略：

- 分阶段上线：`plugin opt-in` -> `灰度` -> `默认可用`。
- 强制保留回退开关：后端回退到 `native`，链路回退到 `novovm`。
- 每次链 profile 扩展必须附带正负向证据。
- 对重叠能力采用“先比对、后切换”策略，禁止无证据直接替换。

## 9. 产物清单

- 本方案文档：`docs_CN/Adapters/EVM/NOVOVM-EVM-ADAPTER-MIGRATION-PLAN-2026-03-06.md`
- 迁移台账：`docs_CN/Adapters/EVM/NOVOVM-EVM-ADAPTER-MIGRATION-LEDGER-2026-03-06.md`
- 统一账户映射规范：`docs_CN/Adapters/EVM/NOVOVM-UNIFIED-ACCOUNT-AND-EVM-PERSONA-MAPPING-SPEC-2026-03-06.md`
- 多链原子协调层规范：`docs_CN/Adapters/EVM/NOVOVM-ATOMIC-ORCHESTRATION-LAYER-SPEC-2026-03-06.md`
- WEB30↔EVM 语义映射矩阵：`docs_CN/Adapters/EVM/NOVOVM-WEB30-EVM-SEMANTIC-MAPPING-MATRIX-2026-03-06.md`
- Ethereum 2026 兼容基线：`docs_CN/Adapters/EVM/NOVOVM-ETHEREUM-PROFILE-2026-COMPAT-BASELINE-2026-03-06.md`
- 临时记录目录：`docs_CN/Adapters/EVM/TEMP-LOG/`

## 10. 与现有台账口径对齐说明

- 状态枚举沿用：`NotStarted / InProgress / ReadyForMerge / Blocked / Done`。
- 证据组织沿用：`artifacts/migration/...`。
- 结论口径沿用：`ReadyForMerge / SnapshotGreen`。
