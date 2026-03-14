param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$DraftReportPath = "",
    [string]$FinalReportPath = "",
    [string]$ReadinessSummaryPath = "",
    [switch]$NoRefreshReadinessGate,
    [switch]$NoThrow
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\ga-closure-promotion"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if (-not $DraftReportPath) {
    $DraftReportPath = Join-Path $RepoRoot "docs_CN\SVM2026-MIGRATION\NOVOVM-GA-CLOSURE-REPORT-DRAFT-2026-03-13.md"
} elseif (-not [System.IO.Path]::IsPathRooted($DraftReportPath)) {
    $DraftReportPath = Join-Path $RepoRoot $DraftReportPath
}
$DraftReportPath = [System.IO.Path]::GetFullPath($DraftReportPath)

if (-not $FinalReportPath) {
    $FinalReportPath = Join-Path $RepoRoot "docs_CN\SVM2026-MIGRATION\NOVOVM-GA-CLOSURE-REPORT-2026-03-13.md"
} elseif (-not [System.IO.Path]::IsPathRooted($FinalReportPath)) {
    $FinalReportPath = Join-Path $RepoRoot $FinalReportPath
}
$FinalReportPath = [System.IO.Path]::GetFullPath($FinalReportPath)

if (-not $ReadinessSummaryPath) {
    $ReadinessSummaryPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-release-readiness-gate\week4-release-readiness-summary.json"
} elseif (-not [System.IO.Path]::IsPathRooted($ReadinessSummaryPath)) {
    $ReadinessSummaryPath = Join-Path $RepoRoot $ReadinessSummaryPath
}
$ReadinessSummaryPath = [System.IO.Path]::GetFullPath($ReadinessSummaryPath)

$readinessScript = Join-Path $RepoRoot "scripts\migration\run_week4_release_readiness_gate.ps1"
if (-not $NoRefreshReadinessGate) {
    if (-not (Test-Path $readinessScript)) {
        throw "missing readiness gate script: $readinessScript"
    }
    & $readinessScript -RepoRoot $RepoRoot -NoThrow | Out-Null
}

if (-not (Test-Path $ReadinessSummaryPath)) {
    throw "missing readiness summary json: $ReadinessSummaryPath"
}

if (-not (Test-Path $DraftReportPath)) {
    throw "missing ga closure draft report: $DraftReportPath"
}

$readiness = Get-Content -Path $ReadinessSummaryPath -Raw | ConvertFrom-Json
$now = Get-Date
$decision = [string]$readiness.decision
$pass = [bool]$readiness.pass
$reasons = @($readiness.reasons | ForEach-Object { [string]$_ })
if ($reasons.Count -eq 0) {
    $reasons = @()
}

$finalReportWritten = $false
$finalReportSha256 = ""
$draftSnapshotPath = Join-Path $OutputDir "ga-closure-draft-snapshot-2026-03-13.md"
Copy-Item -Path $DraftReportPath -Destination $draftSnapshotPath -Force

if ($pass) {
    $stability = $readiness.stability_window
    $audit = $readiness.third_party_audit
    $evidenceDate = $now.ToString("yyyy-MM-dd")
    $decisionTime = $now.ToString("yyyy-MM-dd HH:mm zzz")
    $criticalCount = $audit.critical_count
    $highCount = $audit.high_count
    $mediumCount = $audit.medium_count
    $lowCount = $audit.low_count

    $report = @(
        "# NOVOVM GA 收口报告（正式版，$evidenceDate）",
        "",
        "## 1. 发布判定",
        "",
        "- 判定时间：$decisionTime",
        "- 当前结论：GO（允许 GA）",
        "- 判定门禁：run_week4_release_readiness_gate.ps1",
        "- 判定证据：$ReadinessSummaryPath",
        "",
        "## 2. 阻断项解除证明",
        "",
        "- 稳定窗口：pass=$($stability.pass)，process_running=$($stability.process_running)，observed_iterations=$($stability.observed_iterations)/$($stability.min_iterations)，time_progress_pct=$($stability.time_progress_pct)。",
        "- 第三方审计：report_received=$($audit.report_received)，report_parsed=$($audit.report_parsed)，critical_count=$criticalCount，high_count=$highCount，medium_count=$mediumCount，low_count=$lowCount，policy_pass=$($audit.policy_pass)。",
        "- 审计报告路径：$($audit.report_path)",
        "",
        "## 3. 已完成证据（可复现）",
        "",
        "- full_snapshot_ga_v1 快照通过（含经济服务面/运营控制面 gate）：",
        "  - artifacts/migration/week1-2026-03-13/release-snapshot-ga-v1-2026-03-13-1425/release-snapshot.json",
        "- RC 候选已生成（含经济/运营新 gate 字段）：",
        "  - artifacts/migration/week1-2026-03-13/release-candidate-novovm-rc-2026-03-13-ga-v1-econops-1334/rc-candidate.json",
        "- 漏洞响应机制已发布：",
        "  - docs_CN/SVM2026-MIGRATION/NOVOVM-VULNERABILITY-RESPONSE-POLICY-2026-03-13.md",
        "- Week4 阻断看板：",
        "  - artifacts/migration/week1-2026-03-13/week4-blocker-status/week4-blocker-status.json",
        "",
        "## 4. 风险遗留与发布后策略",
        "",
        "- 继续维持第三方漏洞审计常态化轮次；任何 Critical/High 新发现均触发发布阻断。",
        "- 发布后持续运行 Week4 readiness 与稳定性窗口监控，出现停滞或关键门禁失败立即降级并冻结放量。",
        "",
        "## 5. 回退策略（发布异常时）",
        "",
        "1. 冻结 GA 发布与增量变更。",
        "2. 回退到最近 overall_pass=true 的 RC 快照及对应运行配置。",
        "3. 复跑 functional/performance/economic_service_surface/ops_control_surface 四类门禁。",
        "4. 执行资金路径与审计日志完整性复验后再恢复放量。"
    )

    $report -join "`n" | Set-Content -Path $FinalReportPath -Encoding UTF8
    $finalReportWritten = $true
    $finalReportSha256 = (Get-FileHash -Path $FinalReportPath -Algorithm SHA256).Hash.ToLowerInvariant()
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    generated_at_local = $now.ToString("yyyy-MM-dd HH:mm:ss zzz")
    profile = "ga_closure_promotion_v1"
    decision = $decision
    pass = $pass
    promoted = $finalReportWritten
    reasons = $reasons
    readiness_summary_json = $ReadinessSummaryPath
    blocker_status_json = [string]$readiness.blocker_status_json
    draft_report_path = $DraftReportPath
    draft_snapshot_path = $draftSnapshotPath
    final_report_path = $FinalReportPath
    final_report_sha256 = $finalReportSha256
}

$summaryJson = Join-Path $OutputDir "ga-closure-promotion-summary.json"
$summaryMd = Join-Path $OutputDir "ga-closure-promotion-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# GA Closure Promotion Summary"
    ""
    "- generated_at_local: $($summary.generated_at_local)"
    "- profile: $($summary.profile)"
    "- decision: $($summary.decision)"
    "- pass: $($summary.pass)"
    "- promoted: $($summary.promoted)"
    "- reasons: $([string]::Join(', ', $summary.reasons))"
    "- readiness_summary_json: $($summary.readiness_summary_json)"
    "- blocker_status_json: $($summary.blocker_status_json)"
    "- draft_report_path: $($summary.draft_report_path)"
    "- draft_snapshot_path: $($summary.draft_snapshot_path)"
    "- final_report_path: $($summary.final_report_path)"
    "- final_report_sha256: $($summary.final_report_sha256)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "ga closure promotion summary:"
Write-Host "  decision: $($summary.decision)"
Write-Host "  pass: $($summary.pass)"
Write-Host "  promoted: $($summary.promoted)"
Write-Host "  reasons: $([string]::Join(', ', $summary.reasons))"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"
Write-Host "  final_report_path: $($summary.final_report_path)"

if (-not $summary.promoted -and -not $NoThrow) {
    throw "ga closure promotion not executed: release readiness is NO-GO ($([string]::Join(', ', $summary.reasons)))"
}

if ($summary.promoted) {
    Write-Host "ga closure report promoted to final"
} else {
    Write-Host "ga closure report remains draft (NoThrow=$NoThrow)"
}
