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
- 应用生态能力（F-11~F-13）以受限主链路迁入（治理+门禁收口，默认安全策略不放宽）。

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
- 已完成：`novovm-node` 增加 `NOVOVM_NODE_MODE=rpc_server`（读查询 RPC：`getBlock/getTransaction/getReceipt/getBalance`），并新增 `run_chain_query_rpc_gate.ps1`；该门禁已接入 `run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `chain_query_rpc_pass` 约束，且包含 `rate_limit_signal`(429/`-32029`)）。
- 已完成：`novovm-node` 增加 `NOVOVM_NODE_MODE=header_sync_probe`（headers-first 同步探针），并新增 `run_header_sync_gate.ps1`（`header_sync_signal` + `header_sync_negative_signal`）；该门禁已接入 `run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `header_sync_pass` 约束）。
- 已完成：`novovm-node` 增加 `NOVOVM_NODE_MODE=fast_state_sync_probe`（fast headers + state snapshot verify），并新增 `run_fast_state_sync_gate.ps1`（`fast_state_sync_signal` + `fast_state_sync_negative_signal`）；该门禁已接入 `run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `fast_state_sync_pass` 约束）。
- 已完成：`novovm-node` 增加 `NOVOVM_NODE_MODE=network_dos_probe`（peer-score/ban + invalid-block-storm 模拟），并新增 `run_network_dos_gate.ps1`（`network_dos_signal`）；该门禁已接入 `run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `network_dos_pass` 约束）。
- 已完成：`novovm-node` 增加 `NOVOVM_NODE_MODE=pacemaker_failover_probe`（leader 超时失效 -> view-change -> 新 leader 提案/投票/QC/commit），并新增 `run_pacemaker_failover_gate.ps1`；该门禁已接入 `run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `pacemaker_failover_pass` 约束）。
- 已完成：新增 `run_slash_governance_gate.ps1` 并接入 `run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `slash_governance_pass` 约束），用于验证 `SlashPolicy(mode/threshold/min_active)` 策略行为。
- 已完成：新增默认外置配置 `config/novovm-consensus-policy.json`，`novovm-node` 启动时读取并输出 `slash_policy_in`（缺失时回落默认策略，支持 UTF-8 BOM）。
- 已完成：`novovm-node` 增加 `NOVOVM_NODE_MODE=slash_policy_probe`，用于验证外置 `SlashPolicy` 注入到 `BFTEngine`（输出 `slash_policy_probe_out: injected=true`）。
- 已完成：新增 `run_slash_policy_external_gate.ps1` 并接入 `run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `slash_policy_external_pass` 约束），覆盖 policy 外置化正向与 `policy_invalid/policy_parse_failed` 负向门禁。
- 已完成：`novovm-consensus` 罚没恢复最小闭环：`SlashPolicy.cooldown_epochs` + 自动解禁（`state.height >= jailed_until_epoch`），`SlashExecution` 已输出 `jailed_until_epoch/cooldown_epochs`。
- 已完成：新增 `run_unjail_cooldown_gate.ps1` 并接入 `run_migration_acceptance_gate.ps1`（`overall_pass` 新增 `unjail_cooldown_pass` 约束），覆盖“未到期拒绝 + 到期自动解禁”门禁。
- 已完成：新增 `run_testnet_bootstrap_gate.ps1`（本地多节点 testnet bootstrap 闭环），并接入 `run_migration_acceptance_gate.ps1` 可选聚合（`IncludeTestnetBootstrapGate`，`overall_pass` 新增 `testnet_bootstrap_pass` 约束）。
- 已完成：RPC 查询门禁稳健性修复：`run_chain_query_rpc_gate.ps1` 兼容 PowerShell 无 `ConvertFrom-Json -Depth` 参数；`novovm-node` query-db 读取兼容 UTF-8 BOM，默认 acceptance 全链路恢复稳定。
- 已完成：自动台账回填脚本 `generate_capability_ledger_auto.ps1`，可基于最新报告生成当日台账快照。
- 已完成：`F-03/F-04` 最小协议骨架 `crates/novovm-protocol`（`ids/messages/wire/protocol_catalog`）迁移起步，可作为网络与共识共享协议类型入口。
- 已完成：`F-05` 迁移骨架 `crates/novovm-consensus`（来自 `supervm-consensus` 能力迁移起点），已通过本地测试。
- 已完成：`F-05` 主网化增量门禁：stake-weighted quorum（按权重收敛）+ QC 声明权重防伪 + equivocation 检测与 slash evidence 记录，且已纳入 `consensus_negative_smoke` 的 `pass` 判定。
- 已完成：`F-05` 继续收口：`view-change`（超时换主）+ `fork-choice`（高度/权重优先）已接入 `novovm-consensus`，并纳入 `consensus_negative_smoke` 的 `pass` 判定。
- 已完成：`F-05` 罚没执行策略（slash execution）已接入（含 jailed + active quorum 重算 + 被罚节点投票/提案拒绝），并纳入 `consensus_negative_smoke` 的 `pass` 判定（`weighted_quorum + equivocation + slash_execution + view_change + fork_choice`）。
- 已完成：`F-05` 罚没治理参数化收口：`SlashPolicy` 已接入 `novovm-consensus`（`mode=enforce|observe_only`、`equivocation_threshold`、`min_active_validators`、`cooldown_epochs`），`slash execution` 已输出治理元数据（`policy_mode/evidence_count/threshold/jailed_until_epoch/cooldown_epochs`）；`consensus_negative_smoke` 的 `pass` 已扩展绑定 `slash_threshold + slash_observe_only + unjail_cooldown` 子项。
- 已完成：`F-05` 交易编解码契约收口到 `novovm-protocol::tx_wire`（`novovm_local_tx_wire_v1`），`novovm-node` 已改为通过协议层 codec 执行 tx wire roundtrip。
- 已完成：`novovm-node` 接入 Batch A 闭环并升级为“真实交易编解码 + mempool 准入 + 多批次输入”（tx ingress -> `tx_codec` -> `mempool_out` -> tx metadata verify -> ops_v2 -> batch partition -> proposal -> vote -> QC -> commit -> block_out -> commit_out），并在功能一致性报告增加 `tx_codec_signal` / `mempool_admission_signal` / `tx_metadata_signal` / `batch_a_input_profile` / `batch_a_closure` / `block_output_signal` / `commit_output_signal` 观测字段（最新口径：`accounts=2`、`fee=1~5`、`demo_txs=8`、`target_batches=2`、`block_out.batches=2`）。
- 已完成：`F-07` 的 `l4-network` 文档测试收口（`cargo test -p l4-network --doc` 与全量测试通过）。
- 已完成：`F-07` 迁移骨架 `crates/novovm-network`（来自 `supervm-network`），并新增 `UdpTransport`；`novovm-node` 的网络探针已改为调用网络层 UDP 传输，且 `run_network_two_process.ps1` 已升级为 N 节点 mesh 探针（`NodeCount=3`，`Rounds=2`，`pairs=6/6`，`directed=12/12`）；`novovm-network` 新增 `udp_transport_mesh_three_nodes_closure` 回归样本，功能一致性报告 `network_process_signal` 已覆盖该口径（`functional-smoke33-native` / `functional-smoke34-plugin` 均通过）；并新增跨进程 `network_block_wire` 验证（`sync payload`= `novovm_block_header_wire_v1`，接收端执行 `consensus binding` 校验），证据 `network-two-process-smoke50` 通过（`block_wire=12/12`）。
- 已完成：`F-07` 网络层 `block wire` 负例门禁（`smoke51`）：`run_network_two_process.ps1` 新增 `TamperBlockWireMode`（`hash_mismatch/class_mismatch/codec_corrupt`），`run_functional_consistency.ps1` 新增 `network_block_wire_negative_signal`；当前证据显示正常路径通过（`network-two-process-smoke51-normal`）且篡改路径必失败（`network-two-process-smoke51-negative`，`block_wire=0/2`），并已纳入功能一致性总门禁（`functional-smoke51-network-wire-negative` 通过）。
- 已完成：`F-07` 网络级 pacemaker 收口：`novovm-protocol` 新增 `Pacemaker(ViewSync/NewView)`，`novovm-node` 的 in-memory/UDP 网络探针均接入 `view_sync/new_view` 收敛；`run_network_two_process.ps1` 与 `run_functional_consistency.ps1` 已把 `network_pacemaker_signal`、`view_sync/new_view` 通过率纳入 `pass` 判定（证据：`artifacts/migration/network-two-process-pacemaker/network-two-process.json`、`artifacts/migration/functional-pacemaker/functional-consistency.json`）。
- 已完成：`F-08` 迁移为双后端（native-first + plugin-optional）：`novovm-adapter-api` 仅保留 IR + Trait 契约；新增原生后端 crate `novovm-adapter-novovm`（`create_native_adapter`）与插件样例 crate `novovm-adapter-sample-plugin`（C ABI: `novovm_adapter_plugin_*`）；`novovm-node` 的 `adapter_out` 已改为按 `NOVOVM_ADAPTER_BACKEND=auto|native|plugin` + `NOVOVM_ADAPTER_CHAIN` 选择执行，并可通过 `NOVOVM_ADAPTER_PLUGIN_PATH` 加载插件；新增插件 ABI 门禁配置 `NOVOVM_ADAPTER_PLUGIN_EXPECT_ABI` / `NOVOVM_ADAPTER_PLUGIN_REQUIRE_CAPS`，并新增注册表门禁 `NOVOVM_ADAPTER_PLUGIN_REGISTRY_PATH` / `NOVOVM_ADAPTER_PLUGIN_REGISTRY_STRICT` / `NOVOVM_ADAPTER_PLUGIN_REGISTRY_SHA256`（配合 registry `allowed_abi_versions` 白名单）；新增共识绑定 `adapter_consensus`（`plugin_class=consensus` + `consensus_adapter_hash`），并把 `consensus_adapter_hash` 写入 `block header`（`block_consensus`），提交阶段执行强校验（`commit_consensus`，不匹配拒块）；一致性报告新增 `adapter_consensus_binding_signal`，并补齐 `adapter_plugin_registry_negative_signal`（hash/whitelist mismatch 负例必须失败）；按顺序证据 `smoke48`（protocol 下沉 + 统一验证函数）与 `smoke49`（registry 两类负例）均为通过态。
- 已完成：域级里程碑 `D0~D3 = Done`（MVP 口径），证据见自动台账 `Domain Scan (D0~D3)` 与 acceptance gate（functional/performance/adapter-stability 全通过）。
- 已完成：`run_migration_acceptance_gate.ps1` 新增一键全开参数 `-FullSnapshotProfile`（profile=`full_snapshot_v1`），可显式开启全部 Include* 门禁并输出 profile 字段到 acceptance summary。
- 已完成：新增发布快照聚合脚本 `scripts/migration/run_release_snapshot.ps1`，产出 `release-snapshot.json/md`（包含 `overall_pass`、`enabled_gates`、`key_results.tps_p50`、`allowed_regression_pct` 与证据路径）。
- 已完成：新增发布候选脚本 `scripts/migration/run_release_candidate.ps1`，以 `rc_ref(tag/hash)` 固定一次 `full_snapshot_v1` 复现流程，并输出 `rc-candidate.json/md`（`ReadyForMerge/SnapshotGreen` 状态单据）。
- 已完成：全量门禁发布快照（2026-03-05）：
  - `artifacts/migration/release-snapshot-2026-03-05/release-snapshot.json`
  - `overall_pass=True`，`profile_name=full_snapshot_v1`
  - 关键口径：`core/cpu_batch_stress.p50=24607691.87`，`core/cpu_parity.p50=5947527.35`
  - 关键闭环：`rpc_pass/governance_pass/sync_pass/adapter_pass/dos_pass/consensus_pass=True`
- 已完成：全量门禁发布快照 relfix（2026-03-05，param3 + adapter stability 回归）：
  - `artifacts/migration/release-snapshot-param3-smoke-relfix/release-snapshot.json`
  - `profile_name=full_snapshot_v1`，`overall_pass=True`
  - 关键 gate：`governance_param3_pass=True`，`adapter_stability_pass=True`
  - 状态：`ReadyForMerge / SnapshotGreen`（relfix 后恢复稳定）
  - 根因：relative `OutputDir` + child process `cwd` 变化导致 whitelist negative path drift
  - 修复：在 `scripts/migration/run_functional_consistency.ps1` 与 `scripts/migration/run_adapter_stability_gate.ps1` 将 `OutputDir` 归一化为绝对路径
  - 证据：`artifacts/migration/adapter-stability-relfix-smoke/adapter-stability-summary.json`
- 已完成：`chain_query_rpc_gate` rate-limit retryfix（2026-03-06）：根因是节点限流按 Unix 秒窗口计数，原 gate 的 3 次 probe 偶发跨秒，导致首轮 `limited_ok=False` 抖动；修复为在 `scripts/migration/run_chain_query_rpc_gate.ps1` 中将 rate-limit probe 对齐到下一个秒窗口，并加入小次数自动重试。
- 证据：
  - `artifacts/migration/chain-query-rpc-gate-retryfix-run-1/chain-query-rpc-gate-summary.json` 至 `artifacts/migration/chain-query-rpc-gate-retryfix-run-10/chain-query-rpc-gate-summary.json`（10/10 全绿，`attempts_used=1`）
  - `artifacts/migration/release-snapshot-chain-query-retryfix-run-1/release-snapshot.json`（`overall_pass=True`）
  - `artifacts/migration/release-snapshot-chain-query-retryfix-run-2/release-snapshot.json`（`overall_pass=True`）
- 已完成：经济执行层命名与继承收口：`market_runtime` 统一迁移为 `market_engine`，并复用 `SVM2026/contracts/web30/core` 的 `AMM/CDP/Bond/NAV/TreasuryImpl` 组件作为 NOVOVM 经济执行主链路。
- 已完成：`run_governance_market_policy_gate.ps1` 新增 `engine_output_pass + treasury_output_pass + orchestration_output_pass` 硬门禁，`run_migration_acceptance_gate.ps1` 的 `overall_pass` 已绑定三项子门禁，`run_release_snapshot.ps1` / `run_release_candidate.ps1` 已同步输出关键结果。
- 已完成：GA 正式发布快照（2026-03-06）：
  - `artifacts/migration/release-snapshot-ga-2026-03-06-051653/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`，`overall_pass=True`
  - `key_results.governance_market_policy_engine_pass=True`
  - `key_results.governance_market_policy_treasury_pass=True`
  - `key_results.governance_market_policy_orchestration_pass=True`
- 已完成：GA 正式 RC（`rc_ref=novovm-rc-2026-03-06-ga-v1`）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-v1/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `commit_hash=823a5880e104c96d03e2ab4a8473c9f620ae6413`
- 已完成：GA orchfix 复核快照（2026-03-06）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-orchfix/snapshot/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`，`overall_pass=True`
  - `key_results.governance_market_policy_orchestration_pass=True`
- 已完成：GA orchfix 复核 RC（`rc_ref=novovm-rc-2026-03-06-ga-orchfix`）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-ga-orchfix/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `commit_hash=bac3763192258d5fcb89fc129e2b675d56dbb317`
  - `governance_market_policy_orchestration_pass=True`
- 已完成：治理审计持久化索引（I-GOV 审计可追溯增强）：`novovm-node` 引入 `NOVOVM_GOVERNANCE_AUDIT_DB` + `GovernanceRpcAuditStore(next_seq/events)`，`run_governance_rpc_gate.ps1` 新增 `audit_persist_ok`，并接入 acceptance。
- 证据：
  - `artifacts/migration/governance-rpc-gate-audit-persist-smoke/governance-rpc-gate-summary.json`（`audit_persist_ok=True`）
  - `artifacts/migration/acceptance-gate-governance-audit-persist-smoke/acceptance-gate-summary.json`（`governance_rpc_audit_persist_pass=True`）
- 已完成：发布快照/RC 已纳入治理审计持久化字段：
  - `artifacts/migration/release-snapshot-audit-persist-smoke/release-snapshot.json`（`key_results.governance_rpc_audit_persist_pass=True`）
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-governance-audit-persist-smoke/rc-candidate.json`（`governance_rpc_audit_persist_pass=True`）
- 已完成：治理链内审计索引（I-GOV 审计下沉 + 恢复）：`novovm-consensus` 新增 `GovernanceChainAuditEvent` 并在 `submit/execute/stage` 写入，RPC 提供 `governance_listChainAuditEvents`；节点层新增 `NOVOVM_GOVERNANCE_CHAIN_AUDIT_DB` 持久化并在重启后恢复到共识引擎。
- 已完成：`run_governance_rpc_gate.ps1` 新增 `chain_audit_ok + chain_audit_persist_ok + chain_audit_restart_ok` 强门禁，并保持 acceptance/snapshot/rc 聚合口径（`governance_rpc_chain_audit_pass`）。
- 已完成：治理链审计 root proof 收口：`governance_getPolicy` 与 `governance_listChainAuditEvents` 同步输出 `head_seq/root`，并在 gate 中强校验 `policy_chain_audit_consistency_ok + chain_audit_root_ok + chain_audit_persist_root_ok + chain_audit_restart_root_ok`；acceptance/snapshot/rc 新增聚合字段 `governance_rpc_chain_audit_root_proof_pass`。
- 已完成：治理链审计 root 区块路径锚定：`novovm-protocol::BlockHeaderWireV1` 新增 `governance_chain_audit_root`，`novovm-node` 的 `block_out/commit_out` 已输出该字段；`run_functional_consistency.ps1` 新增跨 `ffi_v2/legacy_compat` 的 root 一致性断言，`run_migration_acceptance_gate.ps1` / `run_release_snapshot.ps1` / `run_release_candidate.ps1` 新增聚合字段 `governance_chain_audit_root_parity_pass`。
- 证据：
  - `artifacts/migration/governance-rpc-gate-chain-audit-smoke/governance-rpc-gate-summary.json`（`chain_audit_ok=True`）
  - `artifacts/migration/governance-rpc-gate-chain-audit-persist-smoke/governance-rpc-gate-summary.json`（`chain_audit_persist_ok=True`, `chain_audit_restart_ok=True`）
  - `artifacts/migration/governance-rpc-gate-chain-audit-root-smoke/governance-rpc-gate-summary.json`（`policy_chain_audit_consistency_ok=True`, `chain_audit_root_ok=True`, `chain_audit_persist_root_ok=True`, `chain_audit_restart_root_ok=True`）
  - `artifacts/migration/release-snapshot-chain-audit-smoke/release-snapshot.json`（`key_results.governance_rpc_chain_audit_pass=True`）
  - `artifacts/migration/release-snapshot-chain-audit-persist-smoke/release-snapshot.json`（`key_results.governance_rpc_chain_audit_persist_pass=True`, `key_results.governance_rpc_chain_audit_restart_pass=True`）
  - `artifacts/migration/release-snapshot-chain-audit-root-smoke/release-snapshot.json`（`key_results.governance_rpc_chain_audit_root_proof_pass=True`）
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-chain-audit-smoke/rc-candidate.json`（`governance_rpc_chain_audit_pass=True`）
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-chain-audit-persist-smoke/rc-candidate.json`（`governance_rpc_chain_audit_persist_pass=True`, `governance_rpc_chain_audit_restart_pass=True`）
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-chain-audit-root-smoke/rc-candidate.json`（`governance_rpc_chain_audit_root_proof_pass=True`）
  - `artifacts/migration/release-snapshot-governance-chain-audit-root-smoke/release-snapshot.json`（`key_results.governance_chain_audit_root_parity_pass=True`）
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-governance-chain-audit-root-anchor-smoke/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_chain_audit_root_parity_pass=True`）
- 已完成：治理签名算法 staged 抽象（I-GOV-04 staged-only）：`governance_sign/governance_vote` 支持 `signature_scheme` 参数，当前仅 `ed25519` 启用；`mldsa87` 请求明确拒绝并落审计事件，形成固定负向门禁；`novovm-consensus` 已新增 `GovernanceVoteVerifier` execute-hook（默认 `ed25519`）并接入治理执行主链路；`novovm-node` 启动新增 `NOVOVM_GOVERNANCE_VOTE_VERIFIER`（`mldsa87` 目前启动即拒绝）。
- 已完成：治理验签器启动门禁收口：`run_governance_rpc_gate.ps1` 新增 `vote_verifier_startup_ok`（默认 `ed25519` 启动配置生效）与 `vote_verifier_staged_reject_ok`（`mldsa87` 启动拒绝）并接入 acceptance/snapshot/rc 聚合。
- 已完成：CI 门禁接线：`.github/workflows/ci.yml` 新增 `governance_rpc_gate`（windows）并将 `vote_verifier_startup_ok + vote_verifier_staged_reject_ok` 设为硬失败条件。
- 已完成：分支保护自动化脚本：`scripts/migration/set_branch_protection_required_checks.ps1`，可将 `Rust checks + Governance RPC gate (vote verifier)` 设为 `main` 必需检查（required checks）。
- 已完成：I-GOV-04 staged 结构下沉：`governance vote verifier` 的 `scheme parse + factory + staged reject` 已从节点层下沉到 `novovm-consensus::governance_verifier`，节点层改为仅调用 `BFTEngine::set_governance_vote_verifier_by_scheme`。
- 已完成：I-GOV-04 staged 二段下沉：`governance_sign/governance_vote` 的 `signature_scheme` 支持判定不再由节点层硬编码，改为调用 `BFTEngine::governance_signature_scheme_supported` + `governance_vote_verifier_scheme`（以共识 active verifier 为准）。
- 已完成：I-GOV-04 staged 三段下沉：`novovm-consensus` 执行治理投票改用 `GovernanceVoteVerifier::verify_with_report`，并将 `verifier/scheme` 写入 `execute=applied` 链内审计事件；`governance_execute` 返回 active `vote_verifier(name/signature_scheme)`。
- 已完成：I-GOV-04 三段下沉聚合收口：`run_migration_acceptance_gate.ps1` 新增 `governance_rpc_vote_verifier_execute_pass` + `governance_rpc_chain_audit_execute_verifier_proof_pass`，并同步到 `run_release_snapshot.ps1` / `run_release_candidate.ps1` 产物。
- 已完成：I-GOV-04 optional execute 接线：新增 `NOVOVM_GOVERNANCE_MLDSA_MODE=aoem_ffi` 可选路径，`mldsa87` 在显式启用时可通过 AOEM-FFI 动态库验签（默认仍 staged-only 拒绝）；启动阶段增加 `aoem_abi_version==1` 与 `aoem_mldsa_supported==1` 校验，动态库默认名按 OS：Windows `aoem_ffi.dll` / Linux `libaoem_ffi.so` / macOS `libaoem_ffi.dylib`。
- 已完成：I-GOV-04 optional execute 门禁化：新增 `run_governance_rpc_mldsa_ffi_gate.ps1`，验证 `submit -> vote(mldsa87) -> execute -> getPolicy` 主链路、`governance_sign(mldsa87)` 本地签名拒绝、AOEM-FFI 启动校验；`run_migration_acceptance_gate.ps1` 已支持 `IncludeGovernanceRpcMldsaFfiGate`，聚合字段 `governance_rpc_mldsa_ffi_pass` / `governance_rpc_mldsa_ffi_startup_pass`。
- 已完成：发布快照/RC 增补 I-GOV-04 可选聚合入口：`run_release_snapshot.ps1` / `run_release_candidate.ps1` 新增 `-IncludeGovernanceRpcMldsaFfiGate` 与 AOEM 路径参数透传，`release-snapshot.json.key_results` 与 `rc-candidate.json` 新增 `governance_rpc_mldsa_ffi_*` 字段（可选开启，不改变默认 `full_snapshot_*` 语义）。
- 证据：
  - `artifacts/migration/release-snapshot-mldsa-optional-smoke/release-snapshot.json`（`enabled_gates.governance_rpc_mldsa_ffi=True`, `key_results.governance_rpc_mldsa_ffi_pass=True`）
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-mldsa-optional-smoke/rc-candidate.json`（`governance_rpc_mldsa_ffi_gate_enabled=True`, `governance_rpc_mldsa_ffi_pass=True`）
- 证据：
  - `artifacts/migration/governance-rpc-gate-vote-verifier-smoke/governance-rpc-gate-summary.json`（`vote_verifier_startup_ok=True`, `vote_verifier_staged_reject_ok=True`）
  - `artifacts/migration/governance-rpc-gate-downsink-scheme-smoke/governance-rpc-gate-summary.json`（`pass=True`, `sign_unsupported_scheme_reject_ok=True`）
  - `artifacts/migration/governance-rpc-gate-verifier-exec-proof-smoke/governance-rpc-gate-summary.json`（`execute_vote_verifier_ok=True`, `chain_audit_has_execute_applied_verifier=True`, `chain_audit_persist_has_execute_applied_verifier=True`, `chain_audit_restart_has_execute_applied_verifier=True`）
  - `artifacts/migration/release-snapshot-vote-verifier-smoke/release-snapshot.json`（`key_results.governance_rpc_vote_verifier_startup_pass=True`, `key_results.governance_rpc_vote_verifier_staged_reject_pass=True`）
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-vote-verifier-smoke/rc-candidate.json`（`governance_rpc_vote_verifier_startup_pass=True`, `governance_rpc_vote_verifier_staged_reject_pass=True`）
  - `artifacts/migration/release-snapshot-governance-verifier-exec-proof-smoke/release-snapshot.json`（`key_results.governance_rpc_vote_verifier_execute_pass=True`, `key_results.governance_rpc_chain_audit_execute_verifier_proof_pass=True`）
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-governance-verifier-exec-proof-smoke/rc-candidate.json`（`status=ReadyForMerge/SnapshotGreen`, `governance_rpc_vote_verifier_execute_pass=True`, `governance_rpc_chain_audit_execute_verifier_proof_pass=True`）
- 证据：
  - `artifacts/migration/governance-rpc-gate-signature-scheme-smoke/governance-rpc-gate-summary.json`（`sign_unsupported_scheme_reject_ok=True`）
  - `artifacts/migration/acceptance-gate-governance-signature-scheme-smoke/acceptance-gate-summary.json`（`governance_rpc_signature_scheme_reject_pass=True`）
- 已完成：发布快照/RC 已纳入治理签名 staged 字段：
  - `artifacts/migration/release-snapshot-signature-scheme-smoke/release-snapshot.json`（`key_results.governance_rpc_signature_scheme_reject_pass=True`）
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-06-signature-scheme-smoke/rc-candidate.json`（`governance_rpc_signature_scheme_reject_pass=True`）
- 状态：`ReadyForMerge / SnapshotGreen`（`full_snapshot_ga_v1` 主线收口）。
- 已完成：GA 主线回归快照（2026-03-07，post-fix）：
  - `artifacts/migration/release-snapshot-ga-post-fix-2026-03-07/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`，`overall_pass=True`
  - 关键 gate：`governance_market_policy_pass=True`，`governance_market_policy_engine_pass=True`，`governance_market_policy_treasury_pass=True`，`governance_market_policy_orchestration_pass=True`
  - 关键治理扩展：`governance_token_economics_pass=True`，`governance_treasury_spend_pass=True`
  - 性能口径：`core/cpu_batch_stress.p50=23565675.18`，`core/cpu_parity.p50=5797524.92`
- 已完成：GA 多源签名回归快照（2026-03-07，multisig）：
  - `artifacts/migration/release-snapshot-ga-multisig-2026-03-07/release-snapshot.json`
  - `profile_name=full_snapshot_ga_v1`，`overall_pass=True`
  - 治理扩展：`governance_market_policy_dividend_pass=True`，`governance_market_policy_foreign_payment_pass=True`
  - 经济聚合：`economic_pass=True`，`economic_infra_dedicated_pass=True`，`market_engine_treasury_negative_pass=True`，`foreign_rate_source_pass=True`，`nav_valuation_source_pass=True`，`dividend_balance_source_pass=True`
- 已完成：GA 多源签名正式 RC（2026-03-07）：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ga-multisig/rc-candidate.json`
  - `status=ReadyForMerge/SnapshotGreen`
  - `snapshot_profile=full_snapshot_ga_v1`，`snapshot_overall_pass=True`
  - `commit_hash=b72fdd987cf1c61163830bda4d46e4dd34020ecf`
- 已完成：`governance_market_policy` 回归修复（同日重复 `reconfigure` 的 dividend claim 抖动）：
  - 根因：单地址同日重复 claim 导致 `dividend_claims_executed` 偶发为 0，触发 `engine_applied/dividend_output_pass` 抖动。
  - 修复：`novovm-consensus::market_engine` 改为地址环探针（按 `day` 轮转 claim 地址），在保留 `dividend_claims_executed > 0` 严格门禁下恢复稳定。
  - 证据：`artifacts/migration/release-snapshot-ga-post-fix-2026-03-07/acceptance-gate-full/governance-market-policy-gate/governance-market-policy-gate-summary.json`（`dividend_output_pass=True`）。
- 已完成：分红余额源主链路收口（2026-03-07）：
  - `token_runtime` 新增 `dividend_eligible_balances(min_balance)`，`protocol.set_market_governance_policy` 下发到 `market_engine.set_dividend_runtime_balances`。
  - `market_engine` 的 dividend 路径改为“运行态余额注入 + deterministic probe fallback”双轨，新增快照字段 `dividend_runtime_balance_accounts/dividend_eligible_accounts`。
  - 新增专项门禁 `scripts/migration/run_dividend_balance_source_gate.ps1`，并接入 acceptance 聚合字段 `dividend_balance_source_pass`。
  - 证据：`artifacts/migration/dividend-balance-source-gate-2026-03-07/dividend-balance-source-gate-summary.json`（`pass=True`）与 `artifacts/migration/acceptance-economic-dividend-source-smoke-2026-03-07/acceptance-gate-summary.json`（`overall_pass=True`, `dividend_balance_source_pass=True`）。
- 已完成：NAV 估值源主链路收口（2026-03-07）：
  - `market_engine` 新增 `ConfigurableNavValuationSource`，支持 `deterministic/external_feed` 模式、`price_bp` 校验与缺失报价 fallback。
  - `novovm-node` 新增外部 NAV feed 主链路接入：支持 `NOVOVM_GOV_MARKET_NAV_FEED_URL(S)` 多源配置、`NOVOVM_GOV_MARKET_NAV_FEED_MIN_SOURCES` 聚合阈值（中位数）、`NOVOVM_GOV_MARKET_NAV_FEED_SIGNATURE_REQUIRED/KEY` 签名校验，以及 strict/fallback。
  - NAV 快照新增 `nav_valuation_source/nav_valuation_price_bp/nav_valuation_fallback_used`，估值链路可追溯。
  - 新增专项门禁 `scripts/migration/run_nav_valuation_source_gate.ps1`，并接入 acceptance 聚合字段 `nav_valuation_source_pass`。
  - 证据：`artifacts/migration/nav-valuation-source-gate-2026-03-07/nav-valuation-source-gate-summary.json`（`pass=True`）、`artifacts/migration/nav-valuation-source-gate-remote-smoke-2026-03-07/nav-valuation-source-gate-summary.json`（`pass=True`）、`artifacts/migration/nav-valuation-source-gate-multisig-smoke-2026-03-07/nav-valuation-source-gate-summary.json`（`pass=True`）与 `artifacts/migration/acceptance-economic-navfx-dividend-smoke-2026-03-07/acceptance-gate-summary.json`（`overall_pass=True`, `nav_valuation_source_pass=True`）。
- 已完成：ForeignPayment 远端汇率源主链路收口（2026-03-07）：
  - `novovm-node` 新增外部汇率 feed 接入：支持 `NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_URL(S)` 多源配置、`NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_MIN_SOURCES` 聚合阈值（多数聚合）、`NOVOVM_GOV_MARKET_FOREIGN_RATE_FEED_SIGNATURE_REQUIRED/KEY` 签名校验，以及 strict/fallback。
  - `market_engine` 快照新增 `foreign_rate_source/foreign_rate_quote_spec_applied/foreign_rate_fallback_used`，并纳入 `governance_market_policy_probe` 输出信号。
  - `run_foreign_rate_source_gate.ps1` 升级覆盖：远端正向、fallback 正向、strict 负向（含 bad-signature 拒绝）。
  - 证据：`artifacts/migration/foreign-rate-source-gate-remote-smoke-2026-03-07/foreign-rate-source-gate-summary.json`（`pass=True`）、`artifacts/migration/foreign-rate-source-gate-multisig-smoke-2026-03-07/foreign-rate-source-gate-summary.json`（`pass=True`）与 `artifacts/migration/governance-market-policy-gate-forex-smoke-2026-03-07/governance-market-policy-gate-summary.json`（`pass=True`）。
- 已完成：`F-10~F-14` 主线收口并提升为 `Done`（2026-03-07）：
  - `F-10 Web3 storage service`：`chain_query_rpc_pass=True` + `governance_rpc_chain_audit_persist_pass=True` + `governance_rpc_chain_audit_restart_pass=True`
  - `F-11 Domain system`：`governance_access_policy_pass=True` + `governance_council_policy_pass=True` + `governance_execution_pass=True` + `governance_negative_pass=True`
  - `F-12 DeFi core`：`governance_token_economics_pass=True` + `governance_treasury_spend_pass=True` + `governance_market_policy_pass=True`
  - `F-13 adapters multi`：`adapter_non_novovm_sample=True` + `adapter_stability_pass=True` + `F-08 Done`
  - `F-14 vm-runtime split`：`vm_runtime_split_pass=True` + `legacy_vm_runtime_present=False`
  - 证据：`docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-04.md`（`Full Scan Matrix` + `Ledger`）
- 已完成：`F-01~F-08` 由 `ReadyForMerge` 提升为 `Done`（2026-03-07）：
  - 依据：全量 acceptance（`full_snapshot_ga_v1`）持续通过，且核心门禁（functional/performance/rpc/consensus/network/adapter）均 `pass=True`。
  - 证据：`docs_CN/SVM2026-MIGRATION/NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-04.md`（`Full Scan Matrix` + `Domain Scan`）。
- 待推进：AOEM FFI 正式暴露 `state_root` 后，将代理门禁切换为硬一致性校验。
- 已完成：`full_snapshot_v2` 全量 acceptance 回填（2026-03-08）：
  - `artifacts/migration/acceptance-gate-full-v2-2026-03-08/acceptance-gate-summary.json`
  - `profile_name=full_snapshot_v2`，`overall_pass=True`
  - 关键门禁：`governance_param3_pass=True`、`rpc_exposure_pass=True`、`adapter_stability_pass=True`、`vm_runtime_split_pass=True`
  - 性能门禁：`artifacts/migration/acceptance-gate-full-v2-2026-03-08/performance-gate/performance-gate-summary.json`（`pass=True`，`cpu_batch_stress.p50=22974773.70`，`cpu_parity.p50=5011122.19`）
- 已完成：AOEM core + sidecar 基线（含 Network+Consensus Matrix）回填（2026-03-08）：
  - `artifacts/migration/aoem-tps-core-sidecar-2026-03-08-baseline/aoem-core-sidecar-tps-summary.json`
  - `docs_CN/AOEM-FFI/AOEM-FFI-CORE-SIDECAR-TPS-SEAL-2026-03-08.md`
  - `docs_CN/AOEM-FFI/AOEM-FFI-CORE-SIDECAR-TPS-RAW-2026-03-08.csv`
  - `cpu_batch_stress_single` 口径（ops/s）：`core p50/p90/p99=22739572.77/24221285.67/24221285.67`，`persist p50/p90/p99=22212152.71/22244070.84/22244070.84`，`wasm p50/p90/p99=22307735.21/22580958.38/22580958.38`
  - Network+Consensus E2E TPS（recv）口径：`core p50/p90/p99=37.41/37.45/37.45`，`persist p50/p90/p99=37.45/37.50/37.50`，`wasm p50/p90/p99=37.45/37.50/37.50`（`network-two-process.json`）
