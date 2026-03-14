param(
    [string]$RepoRoot = "",
    [string]$SourceReportPath = "",
    [string]$OutputDir = "",
    [string]$CanonicalReportPath = "",
    [string]$Auditor = "",
    [string]$TicketId = "",
    [string]$ExpectedReturnAt = "",
    [switch]$RefreshWeek4Status,
    [switch]$TriggerWeek4Closeout,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $SourceReportPath) {
    throw "SourceReportPath is required"
}
if (-not [System.IO.Path]::IsPathRooted($SourceReportPath)) {
    $SourceReportPath = Join-Path $RepoRoot $SourceReportPath
}
$SourceReportPath = [System.IO.Path]::GetFullPath($SourceReportPath)
if (-not (Test-Path $SourceReportPath)) {
    throw "missing source report: $SourceReportPath"
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\third-party-audit-ingest"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if (-not $CanonicalReportPath) {
    $CanonicalReportPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\third-party-audit-report-2026-03-13.md"
} elseif (-not [System.IO.Path]::IsPathRooted($CanonicalReportPath)) {
    $CanonicalReportPath = Join-Path $RepoRoot $CanonicalReportPath
}
$CanonicalReportPath = [System.IO.Path]::GetFullPath($CanonicalReportPath)

$reportContent = Get-Content -Path $SourceReportPath -Raw
$criticalMatch = [regex]::Match($reportContent, "(?im)^\s*critical_count\s*:\s*(\d+)\s*$")
$highMatch = [regex]::Match($reportContent, "(?im)^\s*high_count\s*:\s*(\d+)\s*$")
$mediumMatch = [regex]::Match($reportContent, "(?im)^\s*medium_count\s*:\s*(\d+)\s*$")
$lowMatch = [regex]::Match($reportContent, "(?im)^\s*low_count\s*:\s*(\d+)\s*$")

if (-not $criticalMatch.Success -or -not $highMatch.Success) {
    throw "report missing required machine fields: critical_count/high_count"
}

$criticalCount = [int]$criticalMatch.Groups[1].Value
$highCount = [int]$highMatch.Groups[1].Value
$mediumCount = if ($mediumMatch.Success) { [int]$mediumMatch.Groups[1].Value } else { $null }
$lowCount = if ($lowMatch.Success) { [int]$lowMatch.Groups[1].Value } else { $null }
$policyPass = ($criticalCount -eq 0 -and $highCount -eq 0)

$appliedCanonicalPath = ""
if (-not $DryRun) {
    $canonicalDir = Split-Path -Parent $CanonicalReportPath
    New-Item -ItemType Directory -Force -Path $canonicalDir | Out-Null
    Copy-Item -Path $SourceReportPath -Destination $CanonicalReportPath -Force
    $appliedCanonicalPath = $CanonicalReportPath
}

$week4StatusJson = ""
$week4StatusMd = ""
if ($RefreshWeek4Status -and -not $DryRun) {
    $statusScript = Join-Path $RepoRoot "scripts\migration\run_week4_blocker_status.ps1"
    if (-not (Test-Path $statusScript)) {
        throw "missing week4 blocker status script: $statusScript"
    }
    & $statusScript -RepoRoot $RepoRoot | Out-Null
    $week4StatusJson = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-blocker-status\week4-blocker-status.json"
    $week4StatusMd = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-blocker-status\week4-blocker-status.md"
}

$readinessSummaryJson = ""
$promotionSummaryJson = ""
$promotionSummaryMd = ""
$promotionDecision = ""
$promotionPromoted = $false
$closeoutSummaryJson = ""
$closeoutSummaryMd = ""
$closeoutClosedOut = $false
if ($TriggerWeek4Closeout -and -not $DryRun) {
    $closeoutScript = Join-Path $RepoRoot "scripts\migration\run_week4_closeout.ps1"
    if (-not (Test-Path $closeoutScript)) {
        throw "missing week4 closeout script: $closeoutScript"
    }

    & $closeoutScript -RepoRoot $RepoRoot -NoThrow | Out-Null

    $readinessSummaryJson = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-release-readiness-gate\week4-release-readiness-summary.json"
    $promotionSummaryJson = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\ga-closure-promotion\ga-closure-promotion-summary.json"
    $promotionSummaryMd = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\ga-closure-promotion\ga-closure-promotion-summary.md"
    $closeoutSummaryJson = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-closeout\week4-closeout-summary.json"
    $closeoutSummaryMd = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-closeout\week4-closeout-summary.md"

    if (Test-Path $closeoutSummaryJson) {
        $closeoutSummary = Get-Content -Path $closeoutSummaryJson -Raw | ConvertFrom-Json
        $promotionDecision = [string]$closeoutSummary.decision
        $promotionPromoted = [bool]$closeoutSummary.promoted
        $closeoutClosedOut = [bool]$closeoutSummary.closed_out
    } elseif (Test-Path $promotionSummaryJson) {
        $promotionSummary = Get-Content -Path $promotionSummaryJson -Raw | ConvertFrom-Json
        $promotionDecision = [string]$promotionSummary.decision
        $promotionPromoted = [bool]$promotionSummary.promoted
    }
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    source_report_path = $SourceReportPath
    canonical_report_path = $CanonicalReportPath
    applied_canonical_path = $appliedCanonicalPath
    dry_run = [bool]$DryRun
    refresh_week4_status = [bool]$RefreshWeek4Status
    trigger_week4_closeout = [bool]$TriggerWeek4Closeout
    auditor = $Auditor
    ticket_id = $TicketId
    expected_return_at = $ExpectedReturnAt
    critical_count = $criticalCount
    high_count = $highCount
    medium_count = $mediumCount
    low_count = $lowCount
    policy_pass = $policyPass
    week4_status_json = $week4StatusJson
    week4_status_md = $week4StatusMd
    readiness_summary_json = $readinessSummaryJson
    promotion_summary_json = $promotionSummaryJson
    promotion_summary_md = $promotionSummaryMd
    promotion_decision = $promotionDecision
    promotion_promoted = $promotionPromoted
    closeout_summary_json = $closeoutSummaryJson
    closeout_summary_md = $closeoutSummaryMd
    closeout_closed_out = $closeoutClosedOut
}

$stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$summaryJson = Join-Path $OutputDir ("third-party-audit-ingest-summary-" + $stamp + ".json")
$summaryMd = Join-Path $OutputDir ("third-party-audit-ingest-summary-" + $stamp + ".md")
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Third-Party Audit Ingest Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- source_report_path: $($summary.source_report_path)"
    "- canonical_report_path: $($summary.canonical_report_path)"
    "- applied_canonical_path: $($summary.applied_canonical_path)"
    "- dry_run: $($summary.dry_run)"
    "- refresh_week4_status: $($summary.refresh_week4_status)"
    "- trigger_week4_closeout: $($summary.trigger_week4_closeout)"
    "- auditor: $($summary.auditor)"
    "- ticket_id: $($summary.ticket_id)"
    "- expected_return_at: $($summary.expected_return_at)"
    "- critical_count: $($summary.critical_count)"
    "- high_count: $($summary.high_count)"
    "- medium_count: $($summary.medium_count)"
    "- low_count: $($summary.low_count)"
    "- policy_pass: $($summary.policy_pass)"
    "- week4_status_json: $($summary.week4_status_json)"
    "- week4_status_md: $($summary.week4_status_md)"
    "- readiness_summary_json: $($summary.readiness_summary_json)"
    "- promotion_summary_json: $($summary.promotion_summary_json)"
    "- promotion_summary_md: $($summary.promotion_summary_md)"
    "- promotion_decision: $($summary.promotion_decision)"
    "- promotion_promoted: $($summary.promotion_promoted)"
    "- closeout_summary_json: $($summary.closeout_summary_json)"
    "- closeout_summary_md: $($summary.closeout_summary_md)"
    "- closeout_closed_out: $($summary.closeout_closed_out)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "third-party audit ingest summary:"
Write-Host "  dry_run: $($summary.dry_run)"
Write-Host "  source_report_path: $($summary.source_report_path)"
Write-Host "  applied_canonical_path: $($summary.applied_canonical_path)"
Write-Host "  critical_count: $($summary.critical_count)"
Write-Host "  high_count: $($summary.high_count)"
Write-Host "  policy_pass: $($summary.policy_pass)"
Write-Host "  trigger_week4_closeout: $($summary.trigger_week4_closeout)"
Write-Host "  promotion_decision: $($summary.promotion_decision)"
Write-Host "  promotion_promoted: $($summary.promotion_promoted)"
Write-Host "  closeout_closed_out: $($summary.closeout_closed_out)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"
