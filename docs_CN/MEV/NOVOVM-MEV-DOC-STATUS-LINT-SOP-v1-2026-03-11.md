# NOVOVM MEV 文档状态 Lint 最短执行 SOP v1（2026-03-11）

## 1. 目的

用最短步骤确认 MEV 文档没有“主线/归档混用”风险。

适用分支：`SUPERVM-MEV`

## 2. 每日执行（1 条命令）

```powershell
pwsh -NoProfile -File scripts/migration/run_mev_docs_status_lint.ps1 -RepoRoot .
```

## 3. 通过标准

查看产物：`artifacts/migration/mev/mev-docs-status-lint-summary.json`

必须同时满足：

1. `pass=true`
2. `summary.main_docs_present == summary.main_docs_total`
3. `summary.archive_docs_present == summary.archive_docs_total`
4. `summary.archive_notice_present == summary.archive_docs_total`
5. `summary.fail_count == 0`

## 4. 失败处理

当脚本报错或 `pass=false`：

1. 修复 `docs_CN/MEV/README.md` 的状态标签（`[MAIN]/[ACTIVE]/[ARCHIVE]`）。
2. 给缺失提示的归档文档顶部补“已切主线”说明。
3. 重新执行第 2 节命令直到通过。

## 5. 备注

本 SOP 只做文档治理，不改变任何业务门禁结论。

## 6. 附录：MEV 依赖 EVM 能力实现状态（便捷视图）

说明：

1. 此处用于便捷查看“SUPERVM 当前是否已实现”。
2. 详细口径以 `NOVOVM-MEV-EVM-UPSTREAM-REQUIRED-CAPABILITY-CHECKLIST-2026-03-11.md` 为准。
3. 状态取值：`已实现` / `部分实现` / `未实现`。

| 能力ID | 能力项 | SUPERVM实现状态 | 当前状态（证据口径） |
|---|---|---|---|
| C-01 | 合约调用执行语义 | 已实现 | 已具备（`contract_call_gate=true`） |
| C-04 | 公网广播提交路径 | 未实现 | 未达标（`public_broadcast_gate=false`） |
| C-02 | pending 事件流 | 部分实现 | 未达 fullstack |
| C-03 | txpool 快照 | 部分实现 | 未达 fullstack 双源 |
| C-05 | 回执/错误码语义 | 部分实现 | 未达标（`HR-E05=false`） |
| C-05(扩) | logs/filter/subscribe 语义 | 部分实现 | 未达标（`HR-E03=false`） |
| C-06 | TxType/Profile 策略位 | 部分实现 | 部分具备（策略位框架已存在） |
| C-07 | UCA 对齐接口 | 部分实现 | 并行对齐中 |
