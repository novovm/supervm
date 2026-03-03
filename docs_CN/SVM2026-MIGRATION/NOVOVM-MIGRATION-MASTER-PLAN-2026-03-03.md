# NOVOVM 总迁移计划（生产版）- 2026-03-03

## 1. 总体策略

- 先文档与契约，后工程迁移。
- 先核心发布链路（D0-D4），后生态能力（D5）。
- “将 `SVM2026` 已验证能力逐项迁入 `SUPERVM` 对应模块”作为最后阶段执行。

## 1.1 执行顺序规则（是否必须严格顺序）

不是所有任务都要严格串行，但**关口任务**必须按顺序。

- 必须顺序（Gate）：
  1. 契约冻结（Phase A）
  2. 基线验证可跑通（Phase B）
  3. 核心骨架连通（Phase C）
  4. 核心能力迁移（Phase D）
  5. 最后做逐项能力迁入（Phase E）
- 可并行（在同一 Gate 内）：
  - 文档完善、台账维护、脚本模板、能力探测字段接入
  - 共识与网络两个子域可并行推进

## 1.2 无人值守执行模式（默认）

从本计划起采用“能先做的先做”模式：

- 我将默认直接执行可确定任务，不逐项等待确认。
- 仅在以下情况暂停并询问你：
  1. 需要不可逆操作（删除/重置/破坏性变更）
  2. 需求冲突（存在两个同等可行但方向相反的方案）
  3. 缺关键输入（无法通过本地仓库推断）

## 2. 阶段划分（建议）

## Phase A：架构冻结（2026-03-04 ~ 2026-03-10）

- 冻结新全景架构（六域）与目标模块命名。
- 冻结核心发布范围与生态范围。
- 冻结执行回执与状态根契约字段。
- 冻结 `ZK/MSM` 能力契约字段（能力探测、回退原因码、指标口径）。

出口门槛：

- 架构文档评审通过（无关键异议）。
- 契约字段被 `novovm-exec`/`novovm-node` 接口接受。

## Phase B：验证基线接通（2026-03-11 ~ 2026-03-20）

- 把 `scripts/migration/run_functional_consistency.ps1` 接上 `state_root`。
- 把 `scripts/migration/run_performance_compare.ps1` 导入 `SVM2026` baseline。
- 形成固定报表模板（功能一致性 + 性能回归 + 稳定性）。
- 增加 `ZK/MSM` 能力探测与回退原因码基线采集。

出口门槛：

- 可一键产出 3 类报告。
- 可比较迁移前后同口径结果。

## Phase C：核心骨架完善（2026-03-21 ~ 2026-04-10）

- 按新模块建立 NOVOVM 核心工程骨架（协议、共识、网络、扩展服务）。
- 保持 `novovm-exec` 为唯一 AOEM 执行入口。
- 完成模块间最小可运行链路（非全功能）。

出口门槛：

- 核心链路可跑通最小交易闭环。
- 没有新增直连 AOEM FFI 的旁路调用。

## Phase D：核心能力迁移（2026-04-11 ~ 2026-05-10）

- 先迁共识/网络/协议接口能力（F-05~F-10）与 `ZK/MSM` 契约能力（F-15/F-16）。
- 每迁一项即完成同项回归与回退验证。
- 不引入应用生态能力（F-11~F-13）。

出口门槛：

- D0-D4 核心发布链路可持续运行。
- 回归报告连续通过。

## Phase E：最后阶段，逐项迁入已验证能力（2026-05-11 ~ 2026-06-05）

- 执行 `SVM2026` 已验证能力逐项迁入（你指定的“最后做”）。
- 每项采用“小步迁移 + 独立验收 + 可回退”。
- 先 B 级能力，再 C 级（高风险项最后）。

出口门槛：

- 能力台账全部有“迁入状态 + 验收证据 + 回退记录”。
- 无阻塞性遗留项。

## Phase F：封盘与发布（2026-06-06 ~ 2026-06-15）

- 版本封盘文档（迁移版本、回退步骤、兼容窗口）。
- 生产发布清单（配置、监控、告警、运维手册）。
- 冻结遗留兼容开关下线计划。

## 3. 风险与控制

| 风险 | 表现 | 控制措施 |
|---|---|---|
| 契约漂移 | 迁移后结果不可对比 | 先冻结回执/状态根/指标契约 |
| 模块倒灌 | 旧代码整体搬运导致继续耦合 | 严禁整体迁 `vm-runtime`，只按能力分拆 |
| 指标失真 | 性能数据不可复现 | 固定脚本、固定输入、固定报告模板 |
| 回退缺失 | 线上故障无法快速撤回 | 每个能力项必须有回退策略 |

## 4. 当下“下一步”执行清单（立即可做）

1. 召开一次架构冻结评审，只确认边界和契约，不讨论实现细节。
2. 定义并补齐 `state_root` 字段贯通路径。
3. 导入 `SVM2026` 性能 baseline 到现有脚本。
4. 建立能力迁移台账模板（一能力一记录）。
5. 冻结 `ZK/MSM` 契约字段，并在 `novovm-exec` 对接能力探测输出。

完成以上 5 项后，再开始 Phase C/D 的工程迁移动作。

## 4.1 已落地进展（2026-03-03）

- 已完成：能力迁移台账模板与首版台账文件。
- 已完成：`novovm-exec` 能力契约标准化输出（含 `ZK/MSM` 字段与兼容推断标记）。
- 已完成：能力契约快照脚本 `dump_capability_contract.ps1`。
- 已完成：`run_functional_consistency.ps1` 与 `run_performance_compare.ps1` 自动附带能力快照到报告 JSON/MD。
- 已完成：`state_root` 代理贯通（`state_root_consistency` 字段入报告，当前 `available=false` 时使用 `proxy_digest` 门禁）。
- 已完成：`SVM2026` baseline 自动导入脚本 `import_svm2026_baseline.ps1`，并接入 `run_performance_compare.ps1`（`-AutoImportSvmBaseline`）。
- 已完成：性能对照口径冻结，`run_performance_compare.ps1` 默认 `release` + `warmup_calls=5`，并新增 `LineProfile`（`default|seal_single|seal_auto`）；新增唯一性能门禁脚本 `run_performance_gate_seal_single.ps1`（固定 `release + seal_single + AOEM 封盘基线`，按 3-run P50 判定门禁）。
- 已完成：新增一键迁移验收门禁入口 `run_migration_acceptance_gate.ps1`，串联 `functional_consistency` + `performance_gate_seal_single` 并输出统一 `overall_pass`。
- 已完成：自动台账回填脚本 `generate_capability_ledger_auto.ps1`，可基于最新报告生成当日台账快照。
- 已完成：`F-03/F-04` 最小协议骨架 `crates/novovm-protocol`（`ids/messages/wire/protocol_catalog`）迁移起步，可作为网络与共识共享协议类型入口。
- 已完成：`F-05` 迁移骨架 `crates/novovm-consensus`（来自 `supervm-consensus` 能力迁移起点），已通过本地测试。
- 已完成：`F-05` 交易编解码契约收口到 `novovm-protocol::tx_wire`（`novovm_local_tx_wire_v1`），`novovm-node` 已改为通过协议层 codec 执行 tx wire roundtrip。
- 已完成：`novovm-node` 接入 Batch A 闭环并升级为“真实交易编解码 + mempool 准入 + 多批次输入”（tx ingress -> `tx_codec` -> `mempool_out` -> tx metadata verify -> ops_v2 -> batch partition -> proposal -> vote -> QC -> commit -> block_out -> commit_out），并在功能一致性报告增加 `tx_codec_signal` / `mempool_admission_signal` / `tx_metadata_signal` / `batch_a_input_profile` / `batch_a_closure` / `block_output_signal` / `commit_output_signal` 观测字段（最新口径：`accounts=2`、`fee=1~5`、`demo_txs=8`、`target_batches=2`、`block_out.batches=2`）。
- 已完成：`F-07` 的 `l4-network` 文档测试收口（`cargo test -p l4-network --doc` 与全量测试通过）。
- 已完成：`F-07` 迁移骨架 `crates/novovm-network`（来自 `supervm-network`），并新增 `UdpTransport`；`novovm-node` 的网络探针已改为调用网络层 UDP 传输，且 `run_network_two_process.ps1` 已升级为 N 节点 mesh 探针（`NodeCount=3`，`Rounds=2`，`pairs=6/6`，`directed=12/12`）；`novovm-network` 新增 `udp_transport_mesh_three_nodes_closure` 回归样本，功能一致性报告 `network_process_signal` 已覆盖该口径（`functional-smoke33-native` / `functional-smoke34-plugin` 均通过）；并新增跨进程 `network_block_wire` 验证（`sync payload`= `novovm_block_header_wire_v1`，接收端执行 `consensus binding` 校验），证据 `network-two-process-smoke50` 通过（`block_wire=12/12`）。
- 已完成：`F-07` 网络层 `block wire` 负例门禁（`smoke51`）：`run_network_two_process.ps1` 新增 `TamperBlockWireMode`（`hash_mismatch/class_mismatch/codec_corrupt`），`run_functional_consistency.ps1` 新增 `network_block_wire_negative_signal`；当前证据显示正常路径通过（`network-two-process-smoke51-normal`）且篡改路径必失败（`network-two-process-smoke51-negative`，`block_wire=0/2`），并已纳入功能一致性总门禁（`functional-smoke51-network-wire-negative` 通过）。
- 已完成：`F-08` 迁移为双后端（native-first + plugin-optional）：`novovm-adapter-api` 仅保留 IR + Trait 契约；新增原生后端 crate `novovm-adapter-novovm`（`create_native_adapter`）与插件样例 crate `novovm-adapter-sample-plugin`（C ABI: `novovm_adapter_plugin_*`）；`novovm-node` 的 `adapter_out` 已改为按 `NOVOVM_ADAPTER_BACKEND=auto|native|plugin` + `NOVOVM_ADAPTER_CHAIN` 选择执行，并可通过 `NOVOVM_ADAPTER_PLUGIN_PATH` 加载插件；新增插件 ABI 门禁配置 `NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI` / `NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS`，并新增注册表门禁 `NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH` / `NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT` / `NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256`（配合 registry `allowed_abi_versions` 白名单）；新增共识绑定 `adapter_consensus`（`plugin_class=consensus` + `consensus_adapter_hash`），并把 `consensus_adapter_hash` 写入 `block header`（`block_consensus`），提交阶段执行强校验（`commit_consensus`，不匹配拒块）；一致性报告新增 `adapter_consensus_binding_signal`，并补齐 `adapter_plugin_registry_negative_signal`（hash/whitelist mismatch 负例必须失败）；按顺序证据 `smoke48`（protocol 下沉 + 统一验证函数）与 `smoke49`（registry 两类负例）均为通过态。
- 待推进：AOEM FFI 正式暴露 `state_root` 后，将代理门禁切换为硬一致性校验。
