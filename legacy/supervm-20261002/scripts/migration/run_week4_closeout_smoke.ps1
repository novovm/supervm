param(
    [string]$RepoRoot = "",
    [string]$OutputDir = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-closeout-smoke"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$runDir = Join-Path $OutputDir ("smoke-" + $stamp)
New-Item -ItemType Directory -Force -Path $runDir | Out-Null

$draftSource = Join-Path $RepoRoot "docs_CN\SVM2026-MIGRATION\NOVOVM-GA-CLOSURE-REPORT-DRAFT-2026-03-13.md"
$checklistSource = Join-Path $RepoRoot "docs_CN\SVM2026-MIGRATION\NOVOVM-OPEN-BUSINESS-SURFACE-CLOSURE-CHECKLIST-2026-03-13.md"
if (-not (Test-Path $draftSource)) { throw "missing draft source: $draftSource" }
if (-not (Test-Path $checklistSource)) { throw "missing checklist source: $checklistSource" }

$draftPath = Join-Path $runDir "NOVOVM-GA-CLOSURE-REPORT-DRAFT-smoke.md"
$checklistPath = Join-Path $runDir "NOVOVM-OPEN-BUSINESS-SURFACE-CLOSURE-CHECKLIST-smoke.md"
$finalReportPath = Join-Path $runDir "NOVOVM-GA-CLOSURE-REPORT-final-smoke.md"
$promotionOutputDir = Join-Path $runDir "ga-closure-promotion"
$closeoutOutputDir = Join-Path $runDir "week4-closeout"
$mockReadinessPath = Join-Path $runDir "mock-week4-release-readiness-summary.json"
$mockBlockerStatusPath = Join-Path $runDir "mock-week4-blocker-status.json"
$mockAuditReportPath = Join-Path $runDir "mock-third-party-audit-report.md"

Copy-Item -Path $draftSource -Destination $draftPath -Force
Copy-Item -Path $checklistSource -Destination $checklistPath -Force
Set-Content -Path $mockBlockerStatusPath -Encoding UTF8 -Value "{`"mock`": true}"
Set-Content -Path $mockAuditReportPath -Encoding UTF8 -Value @(
    "# mock audit report"
    "critical_count: 0"
    "high_count: 0"
    "medium_count: 0"
    "low_count: 0"
) -NoNewline:$false

$mockReadiness = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    profile = "week4_release_readiness_gate_mock"
    pass = $true
    decision = "GO"
    blocker_status_json = $mockBlockerStatusPath
    reasons = @()
    stability_window = [ordered]@{
        resolved = $true
        process_running = $false
        expected_end_local = (Get-Date).ToString("yyyy-MM-dd HH:mm:ss zzz")
        time_progress_pct = 100.0
        iteration_progress_pct = 100.0
        observed_iterations = 72
        min_iterations = 72
        minutes_since_last_iteration = 0.0
        stall_detected = $false
        pass = $true
    }
    third_party_audit = [ordered]@{
        resolved = $true
        report_received = $true
        report_parsed = $true
        critical_count = 0
        high_count = 0
        medium_count = 0
        low_count = 0
        policy_pass = $true
        report_path = $mockAuditReportPath
    }
}
$mockReadiness | ConvertTo-Json -Depth 8 | Set-Content -Path $mockReadinessPath -Encoding UTF8

$closeoutScript = Join-Path $RepoRoot "scripts\migration\run_week4_closeout.ps1"
if (-not (Test-Path $closeoutScript)) {
    throw "missing closeout script: $closeoutScript"
}

& $closeoutScript `
    -RepoRoot $RepoRoot `
    -OutputDir $closeoutOutputDir `
    -ChecklistPath $checklistPath `
    -PromotionOutputDir $promotionOutputDir `
    -DraftReportPath $draftPath `
    -FinalReportPath $finalReportPath `
    -ReadinessSummaryPath $mockReadinessPath `
    -NoRefreshReadinessGate `
    -NoThrow | Out-Null

$closeoutSummaryPath = Join-Path $closeoutOutputDir "week4-closeout-summary.json"
if (-not (Test-Path $closeoutSummaryPath)) {
    throw "missing closeout summary: $closeoutSummaryPath"
}

$closeoutSummary = Get-Content -Path $closeoutSummaryPath -Raw | ConvertFrom-Json
$finalExists = Test-Path $finalReportPath
$checklistContent = Get-Content -Path $checklistPath -Raw
$week4Checked = (
    $checklistContent -match "(?m)^- \[x\] 完成 GA 候选 RC \+ 稳定性窗口" -and
    $checklistContent -match "(?m)^- \[x\] 发布最终收口文档" -and
    $checklistContent -match "(?m)^- \[x\] 完成至少 1 轮第三方漏洞审计"
)
$closeoutRowPresent = ($checklistContent -match "\| Week4 自动关单 \|")
$pass = ([bool]$closeoutSummary.closed_out -and $finalExists -and $week4Checked -and $closeoutRowPresent)

$smokeSummary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    profile = "week4_closeout_smoke_v1"
    pass = $pass
    run_dir = $runDir
    closeout_summary_json = $closeoutSummaryPath
    final_report_path = $finalReportPath
    final_report_exists = $finalExists
    checklist_path = $checklistPath
    week4_items_checked = $week4Checked
    closeout_row_present = $closeoutRowPresent
}

$smokeSummaryJson = Join-Path $OutputDir ("week4-closeout-smoke-summary-" + $stamp + ".json")
$smokeSummaryMd = Join-Path $OutputDir ("week4-closeout-smoke-summary-" + $stamp + ".md")
$smokeSummary | ConvertTo-Json -Depth 8 | Set-Content -Path $smokeSummaryJson -Encoding UTF8

$md = @(
    "# Week4 Closeout Smoke Summary"
    ""
    "- generated_at_utc: $($smokeSummary.generated_at_utc)"
    "- profile: $($smokeSummary.profile)"
    "- pass: $($smokeSummary.pass)"
    "- run_dir: $($smokeSummary.run_dir)"
    "- closeout_summary_json: $($smokeSummary.closeout_summary_json)"
    "- final_report_path: $($smokeSummary.final_report_path)"
    "- final_report_exists: $($smokeSummary.final_report_exists)"
    "- checklist_path: $($smokeSummary.checklist_path)"
    "- week4_items_checked: $($smokeSummary.week4_items_checked)"
    "- closeout_row_present: $($smokeSummary.closeout_row_present)"
)
$md -join "`n" | Set-Content -Path $smokeSummaryMd -Encoding UTF8

Write-Host "week4 closeout smoke summary:"
Write-Host "  pass: $($smokeSummary.pass)"
Write-Host "  run_dir: $($smokeSummary.run_dir)"
Write-Host "  closeout_summary_json: $($smokeSummary.closeout_summary_json)"
Write-Host "  smoke_summary_json: $smokeSummaryJson"
Write-Host "  smoke_summary_md: $smokeSummaryMd"

if (-not $pass) {
    throw "week4 closeout smoke FAILED"
}

Write-Host "week4 closeout smoke PASS"
