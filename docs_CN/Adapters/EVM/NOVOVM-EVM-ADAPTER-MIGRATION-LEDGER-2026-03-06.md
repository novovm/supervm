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

生产优先口径（2026-03-09 修订）：

1. 迁移完成度以生产主线路径是否接线为准，不以 gate 数量为准。
2. compare/gate/snapshot/rc 仅作为回归辅助与发布参考，不单独代表功能已完成。
3. 若文档出现“门禁全绿但主线未接线”，按未完成处理。

## 2. Domain Scan（EVM 专项）

| Domain | Status | Done Criteria | Current Evidence |
|---|---|---|---|
| E0 适配契约域 | Done | `novovm-adapter-api` 契约稳定可用 | `ChainType/ChainAdapter/IR` 已在主线 |
| E1 EVM 核心域 | InProgress | precompiles+gas+execution 按 SUPERVM 结构落地 | `novovm-adapter-evm-core` 已落地 M0 语义基线（profile/precompile/gas/tx policy） |
| E2 插件实现域 | InProgress | EVM 专用插件可被 registry + ABI + caps + hash 治理 | `novovm-adapter-evm-plugin` 首版已落地（Transfer-only 兼容边界） |
| E3 多链 profile 域 | InProgress | ETH/BNB/Polygon/Avalanche profile 完整可用 | 已落地 ETH/BNB/Polygon/Avalanche M0 profile resolver + chain profile signal |
| E4 生产接线与发布域 | InProgress | 主线路径接线完成 + 发布口径可复核 | 已新增 EVM 链级 backend compare（evm/polygon/bnb/avalanche）+ tx_type 语义回归，并接入 acceptance/snapshot/rc 汇总 |
| E5 镜像与重叠优化域 | InProgress | 多协议入口语义边界清晰（web30/eth/btc）+ EVM persona 兼容 + 重叠能力路由策略稳定 | 方案已定义；geth 功能盘点与取舍建议已形成文档 |
| E6 统一账户映射域 | InProgress | UCA↔Persona 地址、签名域、nonce/权限规则可审计可门禁 | 账户映射规范文档已创建 |
| E7 原子协调边界域 | InProgress | 原子协调归属清晰（web30）且 `eth_*` 不越权 | 原子协调规范文档已创建 |
| E8 协议映射与基线域 | InProgress | WEB30↔EVM 映射矩阵 + 2026 profile 基线可执行 | 映射矩阵与基线文档已创建 |

## 3. 能力迁移矩阵（EVM-A01 ~ EVM-A20）

注：表中保留的 `gate/signal` 历史字段仅用于追溯旧记录；当前执行口径统一按“生产主线接线”判断，不以这些字段计完成度。

| ID | Capability | Source (SVM2026) | Target (SUPERVM) | Status | Auto Evidence | Next Production Step | Updated |
|---|---|---|---|---|---|---|---|
| EVM-A01 | Adapter contract base | `supervm-chainlinker-api` | `novovm-adapter-api` | Done | adapter contract 已在主线 | N/A | 2026-03-06 |
| EVM-A02 | Native/Plugin 双后端框架 | `chainlinker + plugins` | `novovm-adapter-novovm` + sample plugin | Done | F-08 `ReadyForMerge` | N/A | 2026-03-06 |
| EVM-A03 | EVM precompiles 迁移 | `aoem-adapter-evm/precompiles` | `novovm-adapter-evm-core` | InProgress | 已提供 profile 级 `active_precompile_set_m0`（ETH/BNB/Polygon/Avalanche） | precompile smoke | 2026-03-07 |
| EVM-A04 | EVM gas 迁移 | `aoem-adapter-evm/gas` | `novovm-adapter-evm-core` | InProgress | 已提供 `estimate_intrinsic_gas_m0` 与低 gas 拒绝语义 | gas parity gate | 2026-03-07 |
| EVM-A05 | EVM execution 迁移 | `aoem-adapter-evm/evm_engine` | `novovm-adapter-evm-core` | InProgress | 已接入 `validate_tx_semantics_m0` 并复用 SUPERVM native 执行路径 | execution semantics gate | 2026-03-07 |
| EVM-A06 | EVM tx translator | 历史 adapter 逻辑 | `TxIR` 映射实现（规划） | InProgress | 已落地 raw envelope + 字段级翻译（0/1/2 识别，3 明确拒绝，4 显式策略），并在 `eth_sendRawTransaction` / `eth_sendTransaction` 输出 `tx_ir_type/tx_ir_data_len` 归一化视图 | tx translator gate | 2026-03-07 |
| EVM-A07 | EVM block translator | 历史 adapter 逻辑 | `BlockIR` 映射实现（规划） | InProgress | 已落地 `translate_raw_evm_block_to_ir_m0`（复用 raw tx translator 生成 `BlockIR`）并补充单测 | block translator gate | 2026-03-07 |
| EVM-A08 | EVM plugin crate | `plugins/evm-linker` | `novovm-adapter-evm-plugin-*` | ReadyForMerge | `crates/novovm-adapter-evm-plugin` 已落地 `apply_v2` + UA self-guard 执行路径，并补齐 plugin-side standalone 持久化/审计闭环（`NOVOVM_ADAPTER_PLUGIN_UA_{STORE,AUDIT}_*`）；默认 `memory + none` 保持性能路径零额外 I/O。standalone rocksdb 冒烟证据：`artifacts/migration/unifiedaccount/plugin-selfguard-standalone-smoke-20260308-001323/plugin-selfguard-standalone-smoke-summary.json` | plugin ABI gate | 2026-03-09 |
| EVM-A09 | Multi-chain profile | `EVM/Polygon/BNB/Avalanche` | chain profile config（规划） | InProgress | 已落地 ETH/BNB/Polygon/Avalanche profile resolver（M0），并新增 `run_evm_chain_profile_signal.ps1`（`artifacts/migration/evm-chain-profile-next/evm_chain_profile_signal.json`） | chain profile gate | 2026-03-07 |
| EVM-A10 | Registry 扩展 | plugin manifests | `novovm-adapter-plugin-registry.json` | InProgress | 已增加 `novovm_adapter_evm_plugin_*` 跨平台条目（evm/polygon/bnb/avalanche） | registry strict/hash gate | 2026-03-07 |
| EVM-A11 | Backend compare（链级） | 历史 compare 口径 | EVM 专项 compare（规划） | InProgress | 已新增 `scripts/migration/run_evm_backend_compare_signal.ps1`（支持 `evm/polygon/bnb/avalanche`），并由 acceptance/snapshot/rc 汇总 `evm_backend_compare_*` 结果 | evm backend compare gate | 2026-03-07 |
| EVM-A12 | RC 收口 | 历史发布流程 | EVM 专项 RC（规划） | ReadyForMerge | 已产出 `artifacts/migration/release-snapshot-next-step/release-snapshot.json`、`artifacts/migration/release-candidate-next-step-4chain-strict-v2/rc-candidate.json`、`artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/rc-candidate.json`（严格口径 `AllowedRegressionPct=-5` 下 `snapshot_overall_pass=true`，`evm_chain_profile_signal_pass=true`，`evm_backend_compare_{evm,polygon,bnb,avalanche}_pass=true`，`evm_tx_type_signal_pass=true`，并验证 UA plugin self-guard + rocksdb 联动场景） | rc-candidate gate | 2026-03-09 |
| EVM-A13 | EVM Persona 交互面 | 历史 EVM 接口能力 | `eth_*` 分支 persona layer（规划） | InProgress | 新增 `eth_getTransactionCount` persona 查询别名（按 binding owner + nonce scope 读取下一 nonce）并纳入 `evm_tx_type_signal`（`node_eth_persona_query_cases`） | persona compat gate | 2026-03-07 |
| EVM-A14 | 功能重叠盘点 | 历史 + 当前能力对比 | P0/P1/P2 分类矩阵（规划） | InProgress | `NOVOVM-GETH-FEATURE-CHECKLIST-AND-ADOPTION-RECOMMENDATIONS-2026-03-06.md` | overlap classify gate | 2026-03-06 |
| EVM-A15 | SUPERVM First 路由 | 迁移后统一路由策略 | capability router policy（规划，限 EVM 分支） | InProgress | 已新增 P0/P1/P2 自动路由策略（`NOVOVM_EVM_OVERLAP_P1_COMPARE_READY`、`NOVOVM_EVM_OVERLAP_P2_FORCE_PLUGIN`），并产出 `scripts/migration/run_overlap_router_signal.ps1` + `artifacts/migration/evm-overlap-next-profile/overlap_router_signal.json` | overlap router gate | 2026-03-07 |
| EVM-A16 | 统一账户映射规范 | 全链唯一账户设计资产 | UCA↔EVM Persona 映射规范（规划） | InProgress | `NOVOVM-UNIFIED-ACCOUNT-AND-EVM-PERSONA-MAPPING-SPEC-2026-03-06.md` | evm_account_behavior_signal | 2026-03-06 |
| EVM-A17 | 原子协调层边界规范 | WEB30 原子能力设计资产 | AOL 规范（规划） | InProgress | `NOVOVM-ATOMIC-ORCHESTRATION-LAYER-SPEC-2026-03-06.md` | atomic_boundary_signal | 2026-03-06 |
| EVM-A18 | WEB30↔EVM 语义映射矩阵 | 统一协议能力清单 | 映射矩阵（规划） | InProgress | `NOVOVM-WEB30-EVM-SEMANTIC-MAPPING-MATRIX-2026-03-06.md` | semantic_matrix_signal | 2026-03-06 |
| EVM-A19 | Ethereum 2026 兼容基线 | 外部链兼容需求 | profile 2026 基线（规划） | InProgress | `NOVOVM-ETHEREUM-PROFILE-2026-COMPAT-BASELINE-2026-03-06.md` | evm_tx_type_signal | 2026-03-06 |
| EVM-A20 | 协议语义门禁扩展 | 现有 gate 体系 | tx_type/receipt/error/filter/reorg/account gate 设计（规划） | InProgress | 已新增 `scripts/migration/run_evm_tx_type_signal.ps1`（输出 `artifacts/migration/evm-next-signal/tx_type_compat_signal.json` 与 `artifacts/migration/evm-next-signal-after-plugin-standalone/tx_type_compat_signal.json`，覆盖 `eth_send*` TxIR 归一化、plugin self-guard v2、eth persona nonce 查询、`eth_getTransactionByHash/eth_getTransactionReceipt` 查询别名、`eth_getLogs` M0 显式拒绝与错误码映射） | protocol_semantics_gate | 2026-03-07 |

## 4. 与 F-08 / F-13 对齐

| Legacy Capability | Current Status | EVM 专项解释 |
|---|---|---|
| F-08 Chain adapter interface | ReadyForMerge | 表示框架闭环已就绪，可承载 EVM 迁移 |
| F-13 Multi-chain plugin capability | InProgress | EVM 外部插件首版已进入实质实施，后续补齐 EVM 语义门禁 |

## 5. 阻塞与风险记录

| ID | Type | Description | Impact | Mitigation | Status |
|---|---|---|---|---|---|
| R-01 | 语义风险 | 历史 EVM 引擎存在演示版口径，不等同生产语义 | 可能影响一致性与状态根 | 先 compatibility，再 hardening | Open |
| R-02 | 回归覆盖缺口 | 缺少 `adapter_expected_chain=evm/bnb/polygon/avalanche` 的完整证据 | 不能判定多链 readiness | 增加链级最小回归套件 | Open |
| R-03 | 治理风险 | 插件升级/替换若无强约束可能影响共识绑定 | 影响上线安全边界 | 强制 registry + hash + ABI caps | Open |
| R-04 | 兼容风险 | 镜像节点 persona 若与目标链 RPC/回执语义不一致 | 用户侧可感知异常 | persona compat 回归 + 回执/错误码回归 | Open |
| R-05 | 路由风险 | 重叠能力过早切到 SUPERVM 路径导致语义漂移 | 功能正确性下降 | P1 双跑比对达标后再切换 | Open |
| R-06 | 账户风险 | UCA 与 Persona 地址关系未固定会导致权限/资产边界错配 | 可能引发资产与权限事故 | 先落账户映射规范再实现 | Open |
| R-07 | 协议污染风险 | 将跨链原子语义暴露到 `eth_*` 会破坏 EVM 预期 | 钱包/SDK 兼容与审计风险上升 | 原子协调仅归属 `web30_*` | Open |
| R-08 | 基线漂移风险 | tx type/blob/type4 策略未显式声明导致链兼容漂移 | 线上行为不可预测 | 建立 2026 profile 基线与最小回归样例 | Open |
| R-09 | 性能风险 | 严格口径 `AllowedRegressionPct=-5` 下初次 seal single 门禁未通过（历史记录）；已通过门禁采样稳定化（preset 冷却）+ seal_single worker 参数优化完成修复 | 曾阻塞“性能不损失”目标签收 | 维持 `release + seal_single` 严格口径，持续监控波动并按需复测 | Mitigated |

## 6. 里程碑记录

注：本节包含历史工程化轨迹（含 gate/snapshot/rc 术语）；这些记录保留用于审计追溯，不代表当前推进策略。

| Date | Milestone | Decision | Evidence | Result |
|---|---|---|---|---|
| 2026-03-06 | EVM 迁移方案初始化 | 采用 `Plugin-first + Core-shared` | `docs_CN/Adapters/EVM/NOVOVM-EVM-ADAPTER-MIGRATION-PLAN-2026-03-06.md` | Accepted |
| 2026-03-06 | 方案补充：镜像节点 + 重叠能力策略 | 明确 `Node Persona` 与 `SUPERVM First` 路由规则 | 同上文档（架构图与策略章节） | Accepted |
| 2026-03-06 | 语义边界澄清 | 明确“内核统一、外观多态”；`web30_*` 保持主链语义，EVM Persona 仅用于 `eth_*` 入口 | 同上文档（4.1 与 4.1.1） | Accepted |
| 2026-03-06 | go-ethereum 功能清单审计 | 形成 geth 功能“需要/不需要”建议并接入 EVM 迁移索引 | `docs_CN/Adapters/EVM/NOVOVM-GETH-FEATURE-CHECKLIST-AND-ADOPTION-RECOMMENDATIONS-2026-03-06.md` | Accepted |
| 2026-03-06 | 生产化补强（账户/原子/映射/基线） | 采纳补强意见并新增四份规范，扩展协议语义回归样例 | `docs_CN/Adapters/EVM/NOVOVM-UNIFIED-ACCOUNT-AND-EVM-PERSONA-MAPPING-SPEC-2026-03-06.md` 等四份文档 | Accepted |
| 2026-03-07 | EVM 外部插件首版落地 | 新增 `novovm-adapter-evm-plugin`，并扩展 registry/matrix 接线 | `crates/novovm-adapter-evm-plugin` + `config/novovm-adapter-plugin-registry.json` | InProgress |
| 2026-03-07 | EVM Core M0 语义骨架落地 | 新增 `novovm-adapter-evm-core`，插件接入 profile/precompile/gas/tx policy 校验 | `crates/novovm-adapter-evm-core` + `crates/novovm-adapter-evm-plugin` | InProgress |
| 2026-03-07 | EVM Raw Tx Translator M0 接线 | `eth_sendRawTransaction` 接入 raw envelope 识别（0/1/2/3/4）与策略化路由提示 | `crates/novovm-adapter-evm-core` + `crates/novovm-node/src/bin/novovm-node.rs` | InProgress |
| 2026-03-07 | EVM Tx 字段级翻译与错误码映射 | M0 落地 type0/1/2 字段解析，接入 nonce/chain_id 一致性校验与 eth 路由错误码映射 | `crates/novovm-adapter-evm-core` + `crates/novovm-node/src/bin/novovm-node.rs` + `scripts/migration/run_evm_tx_type_signal.ps1` | InProgress |
| 2026-03-07 | EVM Raw Tx -> TxIR 归一化接线 | `eth_sendRawTransaction` 在路由阶段复用已解析字段构造 `TxIR`，并回传轻量归一化元信息（`tx_ir_type/tx_ir_data_len`） | `crates/novovm-adapter-evm-core/src/lib.rs` + `crates/novovm-node/src/bin/novovm-node.rs` | InProgress |
| 2026-03-07 | EVM `eth_sendTransaction` 归一化接线 | 非 raw EVM 入口也统一输出 `TxIR` 语义标签（transfer/contract_call/contract_deploy）并复用 hex quantity 解析 | `crates/novovm-node/src/bin/novovm-node.rs` + `scripts/migration/run_evm_tx_type_signal.ps1` | InProgress |
| 2026-03-07 | EVM plugin self-guard 从“声明位”升级到“执行位” | 插件新增 `novovm_adapter_plugin_apply_v2` 与 UA self-guard flag，host 在 `prefer_self_guard` 模式下切换调用 v2 并下发 guard flag | `crates/novovm-adapter-evm-plugin/src/lib.rs` + `crates/novovm-node/src/bin/novovm-node.rs` | InProgress |
| 2026-03-07 | EVM plugin self-guard standalone 持久化/审计闭环 | 插件侧新增独立 store/audit backend（`memory|bincode_file|rocksdb` / `none|jsonl|rocksdb`），默认保持 `memory+none` 防止性能损失 | `crates/novovm-adapter-evm-plugin/src/lib.rs` + `crates/novovm-adapter-evm-plugin/Cargo.toml` | InProgress |
| 2026-03-07 | plugin standalone 改动后语义门禁复测 | 复跑 `evm_tx_type_signal`，确认插件 UA self-guard 新增持久化/审计路径未破坏既有 EVM 入口语义门禁 | `artifacts/migration/evm-next-signal-after-plugin-standalone/tx_type_compat_signal.json` | InProgress |
| 2026-03-07 | EVM Block Translator M0 首版落地 | 新增 raw block -> `BlockIR` 归一化函数，交易翻译复用现有 raw tx translator | `crates/novovm-adapter-evm-core/src/lib.rs` | InProgress |
| 2026-03-07 | EVM Persona 查询别名首版落地 | 新增 `eth_getTransactionCount`（binding owner + nonce scope）查询路径，补充正负向测试 | `crates/novovm-adapter-api/src/unified_account.rs` + `crates/novovm-node/src/bin/novovm-node.rs` | InProgress |
| 2026-03-07 | EVM 链级 Backend Compare M0 首版落地 | 新增 EVM 专项 compare gate，固定 `NOVOVM_ADAPTER_CHAIN=evm` 执行 native/plugin 双路径并比对 `state_root` 一致性 | `scripts/migration/run_evm_backend_compare_signal.ps1` + `artifacts/migration/evm/backend_compare_signal.json` | InProgress |
| 2026-03-07 | EVM Compare 接入 RC/快照主线 | acceptance/snapshot/rc 汇总新增 `evm_backend_compare_*` 字段并纳入总通过判定 | `scripts/migration/run_migration_acceptance_gate.ps1` + `scripts/migration/run_release_snapshot.ps1` + `scripts/migration/run_release_candidate.ps1` | InProgress |
| 2026-03-07 | EVM RC 收口冒烟完成 | 完成 full snapshot + RC 跑通，EVM/BNB compare 进入发布证据链 | `artifacts/migration/release-snapshot-next-step/release-snapshot.json` + `artifacts/migration/release-candidate-next-step/rc-candidate.json` | InProgress |
| 2026-03-07 | EVM receipt/filter/reorg 语义门禁补齐（M0） | 新增 `eth_getTransactionByHash/eth_getTransactionReceipt` 查询别名，并对 `eth_getLogs` 等 filter/reorg 方法输出 M0 固定拒绝码（-32036） | `crates/novovm-node/src/bin/novovm-node.rs` + `scripts/migration/run_evm_tx_type_signal.ps1` + `artifacts/migration/evm-next-signal/tx_type_compat_signal.json` | InProgress |
| 2026-03-07 | EVM Overlap Router（A15）首版落地 | Auto 模式引入 P0/P1/P2 路由顺序策略：P0 默认 native-first，P1 compare 未绿时 plugin-first，P2 默认 plugin-first（可配置回退）并接入 overlap router signal | `crates/novovm-node/src/bin/novovm-node.rs` + `scripts/migration/run_overlap_router_signal.ps1` + `artifacts/migration/evm-overlap-next-profile/overlap_router_signal.json` | InProgress |
| 2026-03-07 | EVM Multi-chain Profile（A09）M0 扩展落地 | profile resolver 扩展到 Polygon/Avalanche，native backend 同步支持 Polygon/Avalanche，新增 `evm_chain_profile_signal` 并接入 acceptance/snapshot/rc 汇总字段 | `crates/novovm-adapter-evm-core/src/lib.rs` + `crates/novovm-adapter-novovm/src/lib.rs` + `crates/novovm-node/src/bin/novovm-node.rs` + `scripts/migration/run_evm_chain_profile_signal.ps1` + `scripts/migration/run_migration_acceptance_gate.ps1` + `scripts/migration/run_release_snapshot.ps1` + `scripts/migration/run_release_candidate.ps1` + `artifacts/migration/evm-chain-profile-next/evm_chain_profile_signal.json` | InProgress |
| 2026-03-07 | EVM Backend Compare 四链接线补齐（A11） | compare gate 与发布汇总扩展到 `polygon/avalanche`，并修复 compare 复跑 nonce 重放（脚本每次重建 backend state 目录） | `scripts/migration/run_evm_backend_compare_signal.ps1` + `scripts/migration/run_migration_acceptance_gate.ps1` + `scripts/migration/run_release_snapshot.ps1` + `scripts/migration/run_release_candidate.ps1` + `artifacts/migration/evm-backend-compare-smoke-v2/polygon/backend_compare_signal.json` + `artifacts/migration/evm-backend-compare-smoke-v2/avalanche/backend_compare_signal.json` | InProgress |
| 2026-03-07 | EVM 四链发布链路全绿（A11/A12） | `acceptance -> snapshot -> rc` 在 `full_snapshot_v2` 下跑通，四链 compare 字段全部通过 | `artifacts/migration/acceptance-gate-next-step/acceptance-gate-summary.json` + `artifacts/migration/release-snapshot-next-step/release-snapshot.json` + `artifacts/migration/release-candidate-next-step-4chain/rc-candidate.json` | InProgress |
| 2026-03-07 | 严格性能口径复核（-5） | 单独执行 seal single 性能门禁（`AllowedRegressionPct=-5`）未通过，当前仍需优化/基线策略决策 | `artifacts/migration/perf-gate-strict-next-step/performance-gate-summary.json` | InProgress |
| 2026-03-07 | 严格性能口径修复（-5） | 性能 compare 增加 preset 冷却（2s）并优化 seal_single worker 参数，strict gate 与 strict RC 均恢复通过 | `scripts/migration/run_performance_compare.ps1` + `scripts/migration/run_performance_gate_seal_single.ps1` + `artifacts/migration/perf-gate-strict-after-ew4-cooldown-next-step/performance-gate-summary.json` + `artifacts/migration/release-candidate-next-step-4chain-strict-v2/rc-candidate.json` | InProgress |
| 2026-03-08 | EVM Backend Compare Windows 路径硬化 | 为 Windows 默认切换到短状态目录（支持 `NOVOVM_EVM_BACKEND_COMPARE_STATE_ROOT` 覆盖），修复 rocksdb 在深路径下 `OPTIONS-*.dbtmp` 创建失败 | `scripts/migration/run_evm_backend_compare_signal.ps1` + `artifacts/migration/evm-backend-compare-selfguard-rocksdb-20260308-000936/backend_compare_signal.json` | InProgress |
| 2026-03-08 | 严格 RC（self-guard + rocksdb）全链复测通过 | 在 `full_snapshot_v2` + `AllowedRegressionPct=-5` 下复测，四链 compare/tx-type/profile/router 与 UA gate 全绿 | `artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/rc-candidate.json` + `artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/snapshot/acceptance-gate-full/acceptance-gate-summary.json` + `artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/snapshot/acceptance-gate-full/performance-gate/performance-gate-summary.json` | InProgress |
| 2026-03-09 | EVM-A08/EVM-A12 收口到可签收 | 以 plugin-side standalone rocksdb 冒烟 + 严格 RC 证据将 A08/A12 升级为 `ReadyForMerge` | `artifacts/migration/unifiedaccount/plugin-selfguard-standalone-smoke-20260308-001323/plugin-selfguard-standalone-smoke-summary.json` + `artifacts/migration/rc-ua-selfguard-rocksdb-20260308-000948/rc-candidate.json` | Accepted |
| 2026-03-09 | 外部入口边界回退与架构收敛 | 回退 `novovm-node` 的 `public_rpc` 入口耦合改动，明确 `novovm-node` 仅保留内部二进制主线；外部 `HTTP/JSON-RPC` 仅允许在边界层组件，对内必须转换为 AOEM/二进制流水线 | `docs_CN/Adapters/EVM/NOVOVM-EXTERNAL-INGRESS-BOUNDARY-AND-BINARY-PIPELINE-ARCH-2026-03-09.md` + `crates/novovm-node/src/bin/novovm-node.rs` | Accepted |
| 2026-03-09 | 生产二进制 ingress 批量接线（无主入口语义漂移） | `novovm-node` 新增通用 `NOVOVM_OPS_WIRE_DIR` 批量消费 `.opsw1` 能力，保持唯一生产 bin 与内部 AOEM 二进制路径，不引入内部 RPC/HTTP | `crates/novovm-node/src/bin/novovm-node.rs` + `docs_CN/Adapters/EVM/NOVOVM-EXTERNAL-INGRESS-BOUNDARY-AND-BINARY-PIPELINE-ARCH-2026-03-09.md` | Accepted |
| 2026-03-09 | 网关到主线一键运行路径落地 | 新增无 gate 包装的运行脚本，边界层写入 `.opsw1` 后由主线生产 bin 批量消费并归档结果 | `scripts/migration/run_gateway_node_pipeline.ps1` | Accepted |
| 2026-03-09 | 最小真实链路冒烟通过 | `ua_createUca -> ua_bindPersona -> eth_sendRawTransaction` 产出 `.opsw1`，随后被 `novovm-node` 目录模式消费并归档 | `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/ingress/done/batch-20260309063719351/ingress-1773009433843-0.opsw1` | Accepted |
| 2026-03-09 | 网关统一账户管理能力补齐 | 边界层补齐 `ua_rotatePrimaryKey / ua_revokePersona / ua_getBindingOwner / ua_setPolicy`，并新增 `eth_getTransactionCount`（UA nonce 直读），EVM 外部入口不再依赖节点内部管理接口 | `crates/novovm-edge-gateway/src/main.rs` | Accepted |
| 2026-03-09 | EVM+WEB30 双入口生产接线补齐（边界层） | 网关新增 `web30_sendRawTransaction / web30_sendTransaction`，与 `eth_sendRawTransaction` 统一编码为 `.opsw1`，仅边界层保留 HTTP/JSON-RPC，对内保持 `novovm-node -> AOEM` 二进制流水线 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/ingress/done/batch-20260309074046040/ingress-1773013144163-0.opsw1` + `artifacts/ingress/done/batch-20260309074046040/ingress-1773013144214-1.opsw1` | Accepted |
| 2026-03-09 | `web30_sendTransaction`（非 raw）并入固定回归证据链 | 固化 web30 非 raw 样例并接入 smoke 默认流，单次运行内打通 `gateway smoke -> pipeline consume` 全链路，形成三笔 `.opsw1`（eth raw + web30 raw + web30 nonraw）归档证据 | `scripts/migration/baselines/gateway-web30-nonraw-regression-sample-v1.json` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309075057706/ingress-1773013857302-0.opsw1` + `artifacts/ingress/done/batch-20260309075057706/ingress-1773013857357-1.opsw1` + `artifacts/ingress/done/batch-20260309075057706/ingress-1773013857437-2.opsw1` | Accepted |
| 2026-03-09 | `eth_sendTransaction`（非 raw）生产接线补齐 | 网关新增 `eth_sendTransaction`，支持非 raw 参数归一化后写入 `.opsw1` 并复用统一 UA 路由；在同次 smoke 中与 `eth_sendRawTransaction/web30_*` 一并进入 pipeline 消费，形成四笔归档证据 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/baselines/gateway-eth-nonraw-regression-sample-v1.json` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309080726585/ingress-1773014846079-0.opsw1` + `artifacts/ingress/done/batch-20260309080726585/ingress-1773014846148-1.opsw1` + `artifacts/ingress/done/batch-20260309080726585/ingress-1773014846204-2.opsw1` + `artifacts/ingress/done/batch-20260309080726585/ingress-1773014846264-3.opsw1` | Accepted |
| 2026-03-09 | `eth_sendTransaction` `tx` 子对象输入形态兼容回归 | 新增 `tx` 子对象样例并接入 smoke 默认流，验证 `params.tx` 内 `chainId/nonce/from/to` 形态也可归一化写入 `.opsw1`；同次链路 `eth raw + eth nonraw + eth nonraw(tx) + web30 raw + web30 nonraw` 共 5 笔全部进入 pipeline 归档 | `scripts/migration/baselines/gateway-eth-nonraw-tx-object-regression-sample-v1.json` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403613-0.opsw1` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403685-1.opsw1` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403751-2.opsw1` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403824-3.opsw1` + `artifacts/ingress/done/batch-20260309083324165/ingress-1773016403882-4.opsw1` | Accepted |
| 2026-03-09 | `eth_sendTransaction` 标准数组参数形态（`params: [tx]`）生产接线补齐 | gateway 补齐对标准 JSON-RPC 数组参数形态的主线兼容，`eth_sendTransaction` 的 `params: [ { ... } ]` 可直接归一化落 `.opsw1` 并与 `eth_sendRawTransaction/web30_*` 在同次 pipeline 统一消费归档（内部仍保持二进制流水线） | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/baselines/gateway-eth-nonraw-array-params-regression-sample-v1.json` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323027-0.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323096-1.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323157-2.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323227-3.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323287-4.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323352-5.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323410-6.opsw1` + `artifacts/ingress/done/batch-20260309084843787/ingress-1773017323482-7.opsw1` | Accepted |
| 2026-03-09 | `eth_getTransactionByHash / eth_getTransactionReceipt` 边界层查询接线补齐 | gateway 新增两条查询入口并复用已接收的 ETH 交易索引返回 `pending` 视图；对外保持 JSON-RPC 兼容，对内主链路仍仅 `opsw1 -> novovm-node -> AOEM` 二进制流水线。smoke 同次验证 13 请求（8 写入 + 2 查询）并完成 pipeline 归档 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017732912-0.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017732986-1.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733054-2.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733118-3.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733257-4.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733326-5.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733388-6.opsw1` + `artifacts/ingress/done/batch-20260309085533799/ingress-1773017733455-7.opsw1` | Accepted |
| 2026-03-09 | gateway ETH 查询索引可选持久化（默认内存） | 新增 `NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND=memory|rocksdb` 与 `NOVOVM_GATEWAY_ETH_TX_INDEX_PATH`，默认 `memory` 保持性能；开启 `rocksdb` 后支持 gateway 重启后 `eth_getTransactionByHash` 命中既有交易索引 | `crates/novovm-edge-gateway/src/main.rs` + `artifacts/migration/unifiedaccount/gateway-eth-tx-index-rocksdb-restart-smoke-summary.json` | Accepted |
| 2026-03-09 | gateway EVM 返回语义收敛（标准 JSON-RPC） | `eth_sendRawTransaction/eth_sendTransaction` 返回值收敛为交易哈希字符串，`eth_getTransactionCount` 收敛为 hex quantity，提升钱包/SDK 直接兼容性；内部仍保持边界层 RPC + 内部二进制流水线分层 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` | Accepted |
| 2026-03-09 | `eth_chainId/net_version` 边界兼容补齐（无内部链路改造） | gateway 补齐 EVM 基础网络查询接口，边界层直接返回链标识，不产生 `.opsw1` 交易写入，保持内部二进制流水线零额外开销 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` | Accepted |
| 2026-03-09 | 插件源码解耦（binary-first 脚本） | EVM 迁移脚本改为“外部插件二进制优先、源码可选”：`run_evm_backend_compare_signal.ps1` 支持 `-PluginPath`/`NOVOVM_{EVM_,ADAPTER_}PLUGIN_PATH` 优先解析，并通过 `AllowPluginBuild`/`NOVOVM_EVM_PLUGIN_ALLOW_BUILD` 控制是否允许源码自动构建；`run_evm_tx_type_signal.ps1` 支持 `AllowPluginSourceTests`/`NOVOVM_EVM_TX_TYPE_SIGNAL_ALLOW_PLUGIN_SOURCE_TESTS`，源码缺失或禁用时插件源码单测自动跳过且不阻断主线信号 | `scripts/migration/run_evm_backend_compare_signal.ps1` + `scripts/migration/run_evm_tx_type_signal.ps1` + `artifacts/migration/evm-tx-type-signal-source-optional-test/tx_type_compat_signal.json` | Accepted |
| 2026-03-09 | `eth_gasPrice/eth_estimateGas` 边界兼容补齐（无内部链路改造） | gateway 补齐 EVM 常用费用查询入口：`eth_gasPrice` 返回默认 gas price（可由 `NOVOVM_GATEWAY_ETH_DEFAULT_GAS_PRICE` 配置），`eth_estimateGas` 返回 M0 intrinsic gas 估算；两者仅在边界层处理，不进入 `.opsw1` | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` | Accepted |
| 2026-03-09 | `eth_getCode/eth_getStorageAt` 边界兼容补齐（无内部链路改造） | gateway 新增两条 EVM 常用只读查询：`eth_getCode`（当前返回 `0x`）与 `eth_getStorageAt`（当前返回 32-byte 零值），用于外部钱包/SDK 兼容；仅边界层处理，不写 `.opsw1`、不进入内部 AOEM 执行主线 | `crates/novovm-edge-gateway/src/main.rs` + `scripts/migration/run_gateway_node_smoke.ps1` + `artifacts/migration/unifiedaccount/gateway-node-eth-web30-nonraw-smoke-pipeline-summary.json` | Accepted |
| 2026-03-09 | EVM 插件实码从 `Transfer-only` 扩展到 `ContractCall + ContractDeploy` | 直接推进插件/核心/native 三层执行能力：`novovm-adapter-evm-plugin` 输入边界放开到 `Transfer|ContractCall|ContractDeploy`，`novovm-adapter-evm-core` 语义校验同步放开，`novovm-adapter-novovm` 增加 call/deploy 执行分支（含 deploy 地址派生与代码哈希落态） | `crates/novovm-adapter-evm-plugin/src/lib.rs` + `crates/novovm-adapter-evm-core/src/lib.rs` + `crates/novovm-adapter-novovm/src/lib.rs` | Accepted |
| 2026-03-09 | AOEM sidecar 插件目录“自适应热插拔”解析补齐 | `novovm-exec` 新增 `NOVOVM_AOEM_PLUGIN_DIRS`/`AOEM_FFI_PLUGIN_DIRS` 目录列表支持，按“匹配度+更新时间”自动选择插件目录（启动期解析，零运行期轮询），兼容插件独立发布/替换 | `crates/novovm-exec/src/lib.rs` | Accepted |

## 7. 更新规则（执行期）

1. 每完成一个工作包，更新 `Status/Updated/Auto Evidence/Next Production Step`。
2. 每次生产主线改动后，补充最小链路证据（`artifacts/ingress/...` 或可复现主线路径产物）；gate 产物仅作辅助。
3. 任何阻塞项必须在 `风险记录` 新增条目并标注 owner。
4. 里程碑决策必须可回溯到文档或产物路径。

