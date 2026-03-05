# NOVOVM（SuperVM）迁移文档索引（2026-03-03）

本文档集用于替代 `SVM2026/ROADMAP.md` 中旧的混合叙事，面向 `NOVOVM` 生产版落地。

## 文档清单

1. `NOVOVM-SVM2026-AUDIT-2026-03-03.md`
   - 对 `SVM2026` 代码与 `ROADMAP.md` 做结构化审计，输出可迁移性结论。
2. `NOVOVM-PANORAMA-ARCH-2026-03-03.md`
   - 给出 NOVOVM 新全景图与分层边界（替代旧五层图）。
3. `NOVOVM-FUNCTION-CATALOG-2026-03-03.md`
   - 给出功能分类、模块归属、迁移方式（复用/重构/暂缓）。
4. `NOVOVM-MIGRATION-MASTER-PLAN-2026-03-03.md`
   - 给出阶段计划、时间窗、验收门槛与风险控制。
5. `SVM2026-LAYER-STATUS-VERIFIED-2026-03-03.md`
   - 对 SVM2026 共识层/网络层做构建与测试核验，形成迁移基线状态。
6. `NOVOVM-CAPABILITY-MIGRATION-LEDGER-TEMPLATE-2026-03-03.md`
   - 能力迁移执行台账模板（按能力项推进、验证、回退）。
7. `NOVOVM-CAPABILITY-MIGRATION-LEDGER-2026-03-03.md`
   - 能力迁移执行台账（实际推进状态）。
8. `NOVOVM-CAPABILITY-MIGRATION-LEDGER-AUTO-2026-03-03.md`
   - 自动回填台账快照（由迁移报告脚本生成）。
9. `NOVOVM-RELEASE-RC-RUNBOOK-2026-03-05.md`
   - 发布候选（RC）流程手册，固定 `full_snapshot_v1` 复现口径，并新增治理 RPC 安全发布铁律与 `full_snapshot_v2`（含 RPC 暴露门禁）入口。

## 建议阅读顺序

1. 审计报告（先确认问题边界）
2. 新全景架构（确认目标形态）
3. 功能分类（确认迁移颗粒度）
4. 总计划（确认执行节奏）

## 本轮冻结结论

- `AOEM` 已是 NOVOVM 的底座，不再把执行内核散落在业务模块中。
- 旧 `SVM2026` 的“核心/共识/应用混合”结构不再沿用。
- “将 `SVM2026` 已验证能力逐项迁入 `SUPERVM` 对应模块”放在最后阶段执行。
