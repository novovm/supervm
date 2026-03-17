# EVM Adapter 文档索引（SUPERVM）

## 1. 当前有效文档（开源对外）

以下文档是当前主线有效口径，发布/接入请优先阅读：

1. 全链路闭环目标（当前执行口径）  
   `NOVOVM-EVM-FULL-LIFECYCLE-CLOSURE-TARGET-2026-03-17.md`
2. 插件配置/启动/使用手册（运维入口）  
   `NOVOVM-EVM-PLUGIN-CONFIG-SETUP-USAGE-2026-03-16.md`
3. EVM 插件边界铁律（目录、边界、性能约束）  
   `NOVOVM-EVM-PLUGIN-BOUNDARY-IRON-LAWS-2026-03-13.md`
4. 全功能镜像节点模式规范（架构目标）  
   `NOVOVM-EVM-FULL-MIRROR-NODE-MODE-SPEC-2026-03-11.md`
5. 外部入口边界与二进制流水线约束  
   `NOVOVM-EXTERNAL-INGRESS-BOUNDARY-AND-BINARY-PIPELINE-ARCH-2026-03-09.md`

## 2. 历史归档文档（仅背景，不作发布依据）

以下文档包含阶段性状态、历史“100%”口径或迁移过程记录，保留用于追溯，不作为当前发布判定：

1. `archive/NOVOVM-EVM-ADAPTER-MIGRATION-LEDGER-2026-03-06.md`
2. `archive/NOVOVM-EVM-ADAPTER-MIGRATION-PLAN-2026-03-06.md`
3. `archive/NOVOVM-EVM-FULL-MIRROR-100P-CLOSURE-CHECKLIST-2026-03-13.md`
4. `archive/NOVOVM-EVM-NATIVE-PROTOCOL-COMPAT-PROGRESS-2026-03-16.md`
5. `archive/NOVOVM-GETH-FEATURE-CHECKLIST-AND-ADOPTION-RECOMMENDATIONS-2026-03-06.md`
6. `archive/NOVOVM-EVM-UPSTREAM-REQUIRED-CAPABILITY-CHECKLIST-2026-03-11.md`
7. `archive/NOVOVM-EVM-ADAPTER-STRICT-V2-MERGE-CHECKLIST-2026-03-07.md`

## 3. 一键闭环自检（推荐）

```powershell
powershell -ExecutionPolicy Bypass -File scripts/migration/run_evm_full_lifecycle_autopilot.ps1 `
  -FreshRlpxProfile -AutopilotDurationMinutes 10 -ExecproofDurationMinutes 10 `
  -IntervalSeconds 5 -WarmupSeconds 6 `
  -SummaryOut artifacts/migration/evm-full-lifecycle-autopilot-summary.json
```

输出总报告：

- `artifacts/migration/evm-full-lifecycle-autopilot-summary.json`

## 4. 文档清理原则

1. 进度判定以“可复现实跑证据”与主线脚本产物为准，不以历史台账百分比为准。
2. 历史文档允许保留，但必须明确标注“归档/非当前判定依据”。
3. 对外开源文档避免绑定个人本机绝对路径和一次性临时流程。

审计记录：

- `NOVOVM-EVM-DOCS-AUDIT-AND-CLEANUP-2026-03-18.md`
