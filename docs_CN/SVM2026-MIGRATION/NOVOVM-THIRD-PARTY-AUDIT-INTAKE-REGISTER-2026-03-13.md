# NOVOVM 第三方漏洞审计受理登记单（2026-03-13）

## 1. 受理信息

- 受理状态：`PendingAck`
- 发起时间（HDT）：`2026-03-13 13:44`
- 审计方：`TBD`
- 受理单号：`TBD`
- 预计回传时间（HDT）：`TBD`
- 委托草稿：
  - `docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-REQUEST-DRAFT-2026-03-13.md`
- 最新阻断状态快照：
  - `artifacts/migration/week1-2026-03-13/week4-blocker-status/week4-blocker-status.json`
  - `artifacts/migration/week1-2026-03-13/week4-blocker-status/week4-blocker-status.md`

## 2. 交付包信息

- 包路径：
  - `artifacts/migration/week1-2026-03-13/third-party-audit-handoff-pack-2026-03-13-1342.tar.gz`
- SHA256：
  - `634d01b3678387b96f8cb22dec75fe1ef4fba6db855962fc848a2f2cc949bdc2`
- SHA256 文件：
  - `artifacts/migration/week1-2026-03-13/third-party-audit-handoff-pack-2026-03-13-1342.tar.gz.sha256.txt`
- Manifest：
  - `artifacts/migration/week1-2026-03-13/third-party-audit-handoff-pack-2026-03-13-1342.tar.manifest.json`
- 包内核心入口：
  - `release-candidate-novovm-rc-2026-03-13-ga-v1-econops-1334/rc-candidate.json`
  - `release-snapshot-ga-v1-2026-03-13-1425/release-snapshot.json`
  - `acceptance-gate-summary.json`
  - `security-scan/cargo-audit.json`

## 3. 审计验收口径

- 必须满足：
  - `Critical=0`
  - `High=0`
- `Medium/Low` 需有处置计划（版本/ETA/负责人）。

## 4. 回传材料清单

- 第三方审计正式报告（PDF/Markdown）
- 漏洞明细清单（级别/影响/复现/修复建议）
- 最终结论页（是否满足 GA 门槛）
- 机器判定字段（用于阻断脚本）：
  - `critical_count: <int>`
  - `high_count: <int>`
  - `medium_count: <int>`
  - `low_count: <int>`
- 回包模板：
  - `docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-REPORT-TEMPLATE-2026-03-13.md`

## 5. 关单条件

1. 报告回传并完成内部复核。
2. 结论满足 `Critical/High=0`。
3. 回填收口清单与 GA 正式收口报告。

## 6. 对外委托模板

- 邮件/工单模板：
  - `docs_CN/SVM2026-MIGRATION/NOVOVM-THIRD-PARTY-AUDIT-REQUEST-TEMPLATE-2026-03-13.md`

## 7. 回包导入命令（收到报告后）

```powershell
pwsh -File scripts/migration/ingest_third_party_audit_report.ps1 `
  -RepoRoot . `
  -SourceReportPath <审计方回包路径> `
  -Auditor "<审计方名称>" `
  -TicketId "<受理单号>" `
  -ExpectedReturnAt "<回包时间>" `
  -RefreshWeek4Status `
  -TriggerWeek4Closeout
```

说明：
- `critical_count/high_count` 必须存在，否则导入脚本会报错。
- 导入成功后会刷新 `week4-blocker-status.json/md`。
- 增加 `-TriggerWeek4Closeout` 后，脚本会立刻触发 Week4 一键关单链路（等价于额外执行 `run_week4_closeout.ps1`，其内部会串起 readiness + 报告晋级 + 清单回填）。

## 8. 交付包重建命令（可复现）

```powershell
pwsh -File scripts/migration/build_third_party_audit_handoff_pack.ps1 -RepoRoot .
```

## 9. Week4 发布就绪门禁命令

```powershell
pwsh -File scripts/migration/run_week4_release_readiness_gate.ps1 `
  -RepoRoot . `
  -NoThrow
```

说明：
- 返回 `decision=GO` 时，表示稳定窗口与第三方审计阻断均已解除。
- 返回 `decision=NO-GO` 时，查看 `reasons` 列表定位阻断项。

## 10. Week4 Watchdog（后台）

启动：

```bash
nohup pwsh -File scripts/migration/run_week4_watchdog.ps1 -RepoRoot . -IntervalMinutes 10 -DurationMinutes 4320 > artifacts/migration/week1-2026-03-13/week4-watchdog.nohup.log 2>&1 &
```

查看：

```bash
tail -f artifacts/migration/week1-2026-03-13/week4-watchdog.nohup.log
```

## 11. Week4 一键关单命令（手动）

```powershell
pwsh -File scripts/migration/run_week4_closeout.ps1 `
  -RepoRoot . `
  -NoThrow
```

说明：
- `closed_out=true` 才表示“正式报告 + Week4 勾选 + 关单证据”全部完成。
- 当仍是 `NO-GO` 时脚本只会产出 `week4-closeout-summary.json/md`，不会误回勾清单。
- 在 `GO` 自动回填前，脚本会先生成主清单备份与哈希留痕（`checklist_backup_path/checklist_sha256_before/after`）。

## 12. Week4 关单链路 Smoke（隔离验证）

```powershell
pwsh -File scripts/migration/run_week4_closeout_smoke.ps1 `
  -RepoRoot .
```

说明：
- 该脚本会在 `artifacts/migration/week1-2026-03-13/week4-closeout-smoke/` 下创建隔离副本，模拟 `GO` 并验证一键关单链路。
- Smoke 不会修改主清单与主正式报告，仅用于验证脚本链路正确性。

## 13. Week4 自动化健康检查

```powershell
pwsh -File scripts/migration/run_week4_automation_healthcheck.ps1 `
  -RepoRoot .
```

说明：
- 用于检查 Week4 自动化链路是否健康（watchdog 单实例、关键摘要是否新鲜、稳定窗进程是否存活且无 stall）。
- `pass=false` 时优先处理 `issues` 列表，再继续推进发布动作。
