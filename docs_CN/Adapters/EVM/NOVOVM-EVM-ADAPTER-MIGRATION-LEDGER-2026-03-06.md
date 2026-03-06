# NOVOVM EVM/Adapter 迁移进度台账（SUPERVM）- 2026-03-06

## 1. 台账说明

用途：

- 记录 `SVM2026 -> SUPERVM` 的 EVM 与多链 adapter 迁移进度。
- 与现有 `NOVOVM-CAPABILITY-MIGRATION-LEDGER` 的状态口径保持一致。

状态定义：

- `NotStarted`：未开始。
- `InProgress`：进行中，已有可验证中间产物。
- `ReadyForMerge`：迁移闭环已达成，可并入主线。
- `Blocked`：存在明确阻塞项。
- `Done`：已完成且稳定维持。

## 2. Domain Scan（EVM 专项）

| Domain | Status | Done Criteria | Current Evidence |
|---|---|---|---|
| E0 适配契约域 | Done | `novovm-adapter-api` 契约稳定可用 | `ChainType/ChainAdapter/IR` 已在主线 |
| E1 EVM 核心域 | NotStarted | precompiles+gas+execution 按 SUPERVM 结构落地 | 仅有历史来源能力，未迁入 |
| E2 插件实现域 | NotStarted | EVM 专用插件可被 registry + ABI + caps + hash 治理 | 当前为 sample plugin 能力 |
| E3 多链 profile 域 | NotStarted | ETH/BNB/Polygon/Avalanche profile 完整可用 | 仅存在兼容矩阵占位 |
| E4 门禁与发布域 | NotStarted | 正负向门禁 + 稳定性 + RC 口径全绿 | 暂无链级 EVM 证据链 |
| E5 镜像与重叠优化域 | InProgress | 多协议入口语义边界清晰（web30/eth/btc）+ EVM persona 兼容 + 重叠能力路由策略稳定 | 方案已定义；geth 功能盘点与取舍建议已形成文档 |
| E6 统一账户映射域 | InProgress | UCA↔Persona 地址、签名域、nonce/权限规则可审计可门禁 | 账户映射规范文档已创建 |
| E7 原子协调边界域 | InProgress | 原子协调归属清晰（web30）且 `eth_*` 不越权 | 原子协调规范文档已创建 |
| E8 协议映射与基线域 | InProgress | WEB30↔EVM 映射矩阵 + 2026 profile 基线可执行 | 映射矩阵与基线文档已创建 |

## 3. 能力迁移矩阵（EVM-A01 ~ EVM-A20）

| ID | Capability | Source (SVM2026) | Target (SUPERVM) | Status | Auto Evidence | Next Gate | Updated |
|---|---|---|---|---|---|---|---|
| EVM-A01 | Adapter contract base | `supervm-chainlinker-api` | `novovm-adapter-api` | Done | adapter contract 已在主线 | N/A | 2026-03-06 |
| EVM-A02 | Native/Plugin 双后端框架 | `chainlinker + plugins` | `novovm-adapter-novovm` + sample plugin | Done | F-08 `ReadyForMerge` | N/A | 2026-03-06 |
| EVM-A03 | EVM precompiles 迁移 | `aoem-adapter-evm/precompiles` | `novovm-adapter-evm-core`（规划） | NotStarted | 无 | precompile smoke | 2026-03-06 |
| EVM-A04 | EVM gas 迁移 | `aoem-adapter-evm/gas` | `novovm-adapter-evm-core`（规划） | NotStarted | 无 | gas parity gate | 2026-03-06 |
| EVM-A05 | EVM execution 迁移 | `aoem-adapter-evm/evm_engine` | `novovm-adapter-evm-core`（规划） | NotStarted | 无 | execution semantics gate | 2026-03-06 |
| EVM-A06 | EVM tx translator | 历史 adapter 逻辑 | `TxIR` 映射实现（规划） | NotStarted | 无 | tx translator gate | 2026-03-06 |
| EVM-A07 | EVM block translator | 历史 adapter 逻辑 | `BlockIR` 映射实现（规划） | NotStarted | 无 | block translator gate | 2026-03-06 |
| EVM-A08 | EVM plugin crate | `plugins/evm-linker` | `novovm-adapter-evm-plugin-*`（规划） | NotStarted | 无 | plugin ABI gate | 2026-03-06 |
| EVM-A09 | Multi-chain profile | `EVM/Polygon/BNB/Avalanche` | chain profile config（规划） | NotStarted | 无 | chain profile gate | 2026-03-06 |
| EVM-A10 | Registry 扩展 | plugin manifests | `novovm-adapter-plugin-registry.json` | NotStarted | 当前仅 sample | registry strict/hash gate | 2026-03-06 |
| EVM-A11 | Backend compare（链级） | 历史 compare 口径 | EVM 专项 compare（规划） | NotStarted | 当前为 novovm 主链 | evm backend compare gate | 2026-03-06 |
| EVM-A12 | RC 收口 | 历史发布流程 | EVM 专项 RC（规划） | NotStarted | 无 | rc-candidate gate | 2026-03-06 |
| EVM-A13 | EVM Persona 交互面 | 历史 EVM 接口能力 | `eth_*` 分支 persona layer（规划） | NotStarted | 无 | persona compat gate | 2026-03-06 |
| EVM-A14 | 功能重叠盘点 | 历史 + 当前能力对比 | P0/P1/P2 分类矩阵（规划） | InProgress | `NOVOVM-GETH-FEATURE-CHECKLIST-AND-ADOPTION-RECOMMENDATIONS-2026-03-06.md` | overlap classify gate | 2026-03-06 |
| EVM-A15 | SUPERVM First 路由 | 迁移后统一路由策略 | capability router policy（规划，限 EVM 分支） | NotStarted | 无 | overlap router gate | 2026-03-06 |
| EVM-A16 | 统一账户映射规范 | 全链唯一账户设计资产 | UCA↔EVM Persona 映射规范（规划） | InProgress | `NOVOVM-UNIFIED-ACCOUNT-AND-EVM-PERSONA-MAPPING-SPEC-2026-03-06.md` | evm_account_behavior_signal | 2026-03-06 |
| EVM-A17 | 原子协调层边界规范 | WEB30 原子能力设计资产 | AOL 规范（规划） | InProgress | `NOVOVM-ATOMIC-ORCHESTRATION-LAYER-SPEC-2026-03-06.md` | atomic_boundary_signal | 2026-03-06 |
| EVM-A18 | WEB30↔EVM 语义映射矩阵 | 统一协议能力清单 | 映射矩阵（规划） | InProgress | `NOVOVM-WEB30-EVM-SEMANTIC-MAPPING-MATRIX-2026-03-06.md` | semantic_matrix_signal | 2026-03-06 |
| EVM-A19 | Ethereum 2026 兼容基线 | 外部链兼容需求 | profile 2026 基线（规划） | InProgress | `NOVOVM-ETHEREUM-PROFILE-2026-COMPAT-BASELINE-2026-03-06.md` | evm_tx_type_signal | 2026-03-06 |
| EVM-A20 | 协议语义门禁扩展 | 现有 gate 体系 | tx_type/receipt/error/filter/reorg/account gate 设计（规划） | InProgress | `NOVOVM-EVM-ADAPTER-MIGRATION-PLAN-2026-03-06.md`（第7节） | protocol_semantics_gate | 2026-03-06 |

## 4. 与 F-08 / F-13 对齐

| Legacy Capability | Current Status | EVM 专项解释 |
|---|---|---|
| F-08 Chain adapter interface | ReadyForMerge | 表示框架闭环已就绪，可承载 EVM 迁移 |
| F-13 Multi-chain plugin capability | NotStarted | EVM 多链插件目标尚未进入实质实施 |

## 5. 阻塞与风险记录

| ID | Type | Description | Impact | Mitigation | Status |
|---|---|---|---|---|---|
| R-01 | 语义风险 | 历史 EVM 引擎存在演示版口径，不等同生产语义 | 可能影响一致性与状态根 | 先 compatibility，再 hardening | Open |
| R-02 | 门禁缺口 | 缺少 `adapter_expected_chain=evm/bnb/polygon/avalanche` 的完整证据 | 不能判定多链 readiness | 增加链级 gate 套件 | Open |
| R-03 | 治理风险 | 插件升级/替换若无强约束可能影响共识绑定 | 影响上线安全边界 | 强制 registry + hash + ABI caps | Open |
| R-04 | 兼容风险 | 镜像节点 persona 若与目标链 RPC/回执语义不一致 | 用户侧可感知异常 | persona compat gate + 回执/错误码回归 | Open |
| R-05 | 路由风险 | 重叠能力过早切到 SUPERVM 路径导致语义漂移 | 功能正确性下降 | P1 双跑比对达标后再切换 | Open |
| R-06 | 账户风险 | UCA 与 Persona 地址关系未固定会导致权限/资产边界错配 | 可能引发资产与权限事故 | 先落账户映射规范再实现 | Open |
| R-07 | 协议污染风险 | 将跨链原子语义暴露到 `eth_*` 会破坏 EVM 预期 | 钱包/SDK 兼容与审计风险上升 | 原子协调仅归属 `web30_*` | Open |
| R-08 | 基线漂移风险 | tx type/blob/type4 策略未显式声明导致链兼容漂移 | 线上行为不可预测 | 建立 2026 profile 基线与 gate | Open |

## 6. 里程碑记录

| Date | Milestone | Decision | Evidence | Result |
|---|---|---|---|---|
| 2026-03-06 | EVM 迁移方案初始化 | 采用 `Plugin-first + Core-shared` | `docs_CN/Adapters/EVM/NOVOVM-EVM-ADAPTER-MIGRATION-PLAN-2026-03-06.md` | Accepted |
| 2026-03-06 | 方案补充：镜像节点 + 重叠能力策略 | 明确 `Node Persona` 与 `SUPERVM First` 路由规则 | 同上文档（架构图与策略章节） | Accepted |
| 2026-03-06 | 语义边界澄清 | 明确“内核统一、外观多态”；`web30_*` 保持主链语义，EVM Persona 仅用于 `eth_*` 入口 | 同上文档（4.1 与 4.1.1） | Accepted |
| 2026-03-06 | go-ethereum 功能清单审计 | 形成 geth 功能“需要/不需要”建议并接入 EVM 迁移索引 | `docs_CN/Adapters/EVM/NOVOVM-GETH-FEATURE-CHECKLIST-AND-ADOPTION-RECOMMENDATIONS-2026-03-06.md` | Accepted |
| 2026-03-06 | 生产化补强（账户/原子/映射/基线） | 采纳补强意见并新增四份规范，扩展协议语义 gate | `docs_CN/Adapters/EVM/NOVOVM-UNIFIED-ACCOUNT-AND-EVM-PERSONA-MAPPING-SPEC-2026-03-06.md` 等四份文档 | Accepted |

## 7. 更新规则（执行期）

1. 每完成一个工作包，更新 `Status/Updated/Auto Evidence/Next Gate`。
2. 每次 gate 运行后，补充 `artifacts/migration/...` 证据路径。
3. 任何阻塞项必须在 `风险记录` 新增条目并标注 owner。
4. 里程碑决策必须可回溯到文档或产物路径。
