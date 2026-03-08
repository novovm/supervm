# UNIFIEDACCOUNT 文档索引（SUPERVM）

## 阅读顺序

1. `NOVOVM-UNIFIED-ACCOUNT-AUDIT-SNAPSHOT-2026-03-06.md`
2. `NOVOVM-UNIFIED-ACCOUNT-MIGRATION-PLAN-AND-IMPLEMENTATION-STEPS-2026-03-06.md`
3. `NOVOVM-UNIFIED-ACCOUNT-MIGRATION-LEDGER-2026-03-06.md`
4. `NOVOVM-UNIFIED-ACCOUNT-SPEC-v1-2026-03-06.md`
5. `NOVOVM-UNIFIED-ACCOUNT-GATE-MATRIX-v1-2026-03-06.md`

## 文档清单

- 迁移审计快照
  - `NOVOVM-UNIFIED-ACCOUNT-AUDIT-SNAPSHOT-2026-03-06.md`
  - 用途：确认 SUPERVM 当前进度与 SVM2026 可迁移资产边界。

- 迁移方案与实施步骤
  - `NOVOVM-UNIFIED-ACCOUNT-MIGRATION-PLAN-AND-IMPLEMENTATION-STEPS-2026-03-06.md`
  - 用途：定义目标架构、实施阶段、路由与门禁口径。

- 迁移进度台账
  - `NOVOVM-UNIFIED-ACCOUNT-MIGRATION-LEDGER-2026-03-06.md`
  - 用途：跟踪 UA-Axx 进度、依赖、风险、里程碑。

- 正式规范
  - `NOVOVM-UNIFIED-ACCOUNT-SPEC-v1-2026-03-06.md`
  - 用途：冻结统一账户模型、唯一性、权限、签名域、nonce 与审计规则。

- 门禁矩阵
  - `NOVOVM-UNIFIED-ACCOUNT-GATE-MATRIX-v1-2026-03-06.md`
  - 用途：定义门禁测试用例、失败级别与证据路径。

- 分层接线图（实现层）
  - `NOVOVM-UNIFIED-ACCOUNT-LAYER-WIRING-v1-2026-03-07.md`
  - 用途：明确 `Ingress -> Account Router -> Adapter/Execution` 的落位与接线点。
  - 最新状态：已接入 `public RPC`、`run_ffi_v2`、`native adapter execute_transaction` 与 `plugin adapter ingress(host-side)` 执行前置路由校验（含 replay/domain gate），统一账户状态存储默认 `rocksdb`（保留 `bincode_file` 兼容），统一账户审计后端默认 `rocksdb`（保留 `jsonl` 兼容），支持 `ua_getAuditEvents(source=sink)` 与 `NOVOVM_NODE_MODE=ua_audit_migrate` 增量迁移，新增 `ua_rotatePrimaryKey` 与 `session_expires_at` 校验，并已落地 UA-G01~UA-G16 自动化 gate。RocksDB store 已进一步拆分为 dedicated CF（`ua_store_state_v2`/`ua_store_audit_v2`），同时保持 default-CF/legacy 双写兼容。新增 plugin self-guard capability 合约位（`0x2`）与性能模式开关（`NOVOVM_UNIFIED_ACCOUNT_PLUGIN_PREFER_SELF_GUARD=true`）。

- 源码审计报告（SVM2026 参考实现）
  - `SVM2026-UNIFIEDACCOUNT-CODE-AUDIT-2026-03-07.md`
  - 用途：记录“可迁模型”和“不可直迁实现”的代码级证据。

- SVM2026 参考文档副本
  - `SVM2026-REFERENCE-2026-03-07-v2/`
  - 用途：保留迁移审计依据与原始参考文档索引。

## 目录约定

- 方案类：`*-PLAN-*`
- 规范类：`*-SPEC-*`
- 台账类：`*-LEDGER-*`
- 审计类：`*-AUDIT-*`
- 门禁类：`*-GATE-MATRIX-*`
- 证据目录：`artifacts/migration/unifiedaccount/`

## Gate 执行

- 脚本：`scripts/migration/run_unified_account_gate.ps1`
- 默认输出：`artifacts/migration/unifiedaccount/`
- 汇总文件：
  - `artifacts/migration/unifiedaccount/unified-account-gate-summary.json`
  - `artifacts/migration/unifiedaccount/unified-account-gate-summary.md`
- Acceptance 集成：`scripts/migration/run_migration_acceptance_gate.ps1 -IncludeUnifiedAccountGate $true`

## 最新验证（2026-03-07）

- 严格 GA 门禁（`AllowedRegressionPct=-5.0`）已通过：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-ga-next-rerun10/rc-candidate.json`
- 本机稳定口径（`AllowedRegressionPct=-7.0`）已通过：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-ga-next-local-perf7/rc-candidate.json`
- 诊断口径（`AllowedRegressionPct=-12.0`）已通过：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-ga-next-diag-perf12-r2/rc-candidate.json`
- 本轮 CF 拆分后严格 GA 门禁（`AllowedRegressionPct=-5.0`）已通过：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-next-rc12/rc-candidate.json`
- 本轮 adapter ingress 接线后严格 GA 门禁（`AllowedRegressionPct=-5.0`）已通过：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-next-rc13-rerun1/rc-candidate.json`
- 本轮 plugin ingress（host-side）接线后严格 GA 门禁（`AllowedRegressionPct=-5.0`）已通过：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-next-rc14-rerun1/rc-candidate.json`
- 本轮 plugin self-guard capability + 性能模式开关后严格 GA 门禁（`AllowedRegressionPct=-5.0`）已通过：
  - `artifacts/migration/release-candidate-novovm-rc-2026-03-07-ua-next-rc15-rerun2/rc-candidate.json`

- 本轮关键工程收敛：
  - ingress 归一化别名已扩展：`eth_sendTransaction`、`web30_sendRawTransaction` 接入统一账户路由。
  - native adapter ingress 已接入统一账户前置路由（`execute_transaction`）。
  - plugin adapter ingress（host-side）已接入统一账户前置路由（plugin apply 前 guard，且在 `ffi_v2` 已预校验场景自动跳过重复 guard）。
  - plugin ingress 新增性能模式：可切换为“plugin self-guard 优先”并自动要求 capability `0x2`，避免 host+plugin 双重校验导致的额外损耗。
  - 统一账户 store 的 RocksDB 已升级为 dedicated CF 分层：`ua_store_state_v2` / `ua_store_audit_v2`，并保持 default-CF namespace + legacy key 双写兼容。
  - foreign/nav source gate 的 fallback 与脚本聚合修复完成。
  - adapter stability gate 已增加 wasm digest / abi-negative 抖动重试。
  - `run_functional_consistency` 已改为短路径且每次运行唯一 `NOVOVM_D2D3_STORAGE_ROOT`，规避 Windows 深路径与 nonce 污染。

## 口径说明

- `SVM2026` 是历史实验参考；`SUPERVM` 是生产主线。
- 统一账户迁移遵循：`迁模型，不迁实验实现`。
- 与 EVM 迁移依赖：统一账户是 `WP-10` 前置任务。
