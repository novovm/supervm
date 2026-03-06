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

## 目录约定

- 方案类：`*-PLAN-*`
- 规范类：`*-SPEC-*`
- 台账类：`*-LEDGER-*`
- 审计类：`*-AUDIT-*`
- 门禁类：`*-GATE-MATRIX-*`
- 证据目录：`artifacts/migration/unifiedaccount/`

## 口径说明

- `SVM2026` 是历史实验参考；`SUPERVM` 是生产主线。
- 统一账户迁移遵循：`迁模型，不迁实验实现`。
- 与 EVM 迁移依赖：统一账户是 `WP-10` 前置任务。
