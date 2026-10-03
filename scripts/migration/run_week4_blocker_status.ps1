param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 2000000)]
    [int]$StabilityWindowPid = 231596,
    [string]$StabilityWindowDir = "",
    [ValidateRange(1, 10080)]
    [int]$StabilityWindowMinutes = 4320,
    [ValidateRange(1, 5000)]
    [int]$StabilityMinIterations = 72,
    [ValidateRange(1, 1440)]
    [int]$StabilityStallThresholdMinutes = 90,
    [string]$AuditIntakeRegister = "",
    [string]$AuditHandoffPackPath = "",
    [string]$AuditReportPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-blocker-status"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if (-not $StabilityWindowDir) {
    $StabilityWindowDir = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\stability-window-72h-r2"
} elseif (-not [System.IO.Path]::IsPathRooted($StabilityWindowDir)) {
    $StabilityWindowDir = Join-Path $RepoRoot $StabilityWindowDir
}
$StabilityWindowDir = [System.IO.Path]::GetFullPath($StabilityWindowDir)

if (-not $AuditIntakeRegister) {
    $AuditIntakeRegister = Join-Path $RepoRoot "docs_CN\SVM2026-MIGRATION\NOVOVM-THIRD-PARTY-AUDIT-INTAKE-REGISTER-2026-03-13.md"
} elseif (-not [System.IO.Path]::IsPathRooted($AuditIntakeRegister)) {
    $AuditIntakeRegister = Join-Path $RepoRoot $AuditIntakeRegister
}
$AuditIntakeRegister = [System.IO.Path]::GetFullPath($AuditIntakeRegister)

if (-not $AuditHandoffPackPath) {
    $AuditHandoffPackPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\third-party-audit-handoff-pack-2026-03-13-1342.tar.gz"
} elseif (-not [System.IO.Path]::IsPathRooted($AuditHandoffPackPath)) {
    $AuditHandoffPackPath = Join-Path $RepoRoot $AuditHandoffPackPath
}
$AuditHandoffPackPath = [System.IO.Path]::GetFullPath($AuditHandoffPackPath)

if (-not $AuditReportPath) {
    $AuditReportPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\third-party-audit-report-2026-03-13.md"
} elseif (-not [System.IO.Path]::IsPathRooted($AuditReportPath)) {
    $AuditReportPath = Join-Path $RepoRoot $AuditReportPath
}
$AuditReportPath = [System.IO.Path]::GetFullPath($AuditReportPath)

$now = Get-Date
$requiredSeconds = [Math]::Round($StabilityWindowMinutes * 60.0, 2)
$iterationCount = 0
$iterationProgressPct = 0.0
$iterationMinReached = $false
$latestIterationSummaryJson = ""
$latestIterationUpdatedLocal = $null
$minutesSinceLastIteration = $null
$stallDetected = $false

if (Test-Path $StabilityWindowDir) {
    $iterationDirs = @(Get-ChildItem -Path $StabilityWindowDir -Directory -Filter "iteration-*" -ErrorAction SilentlyContinue)
    $iterationCount = $iterationDirs.Count
    $iterationProgressPct = [Math]::Round([Math]::Min(100.0, ($iterationCount * 100.0) / [double]$StabilityMinIterations), 3)
    $iterationMinReached = ($iterationCount -ge $StabilityMinIterations)

    $latestSummary = Get-ChildItem -Path $StabilityWindowDir -Recurse -File -Filter "adapter-stability-summary.json" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($null -ne $latestSummary) {
        $latestIterationSummaryJson = $latestSummary.FullName
        $latestIterationUpdatedLocal = $latestSummary.LastWriteTime
    }
}

$processRunning = $false
$processStartLocal = $null
$processStartUtc = $null
$elapsedSeconds = 0.0
$timeProgressPct = 0.0
$expectedEndLocal = $null
$remainingSeconds = $requiredSeconds

try {
    $p = Get-Process -Id $StabilityWindowPid -ErrorAction Stop
    $processRunning = $true
    $processStartLocal = $p.StartTime
    $processStartUtc = $processStartLocal.ToUniversalTime()
    $elapsedSeconds = [Math]::Round(($now - $processStartLocal).TotalSeconds, 2)
    $timeProgressPct = [Math]::Round([Math]::Min(100.0, ($elapsedSeconds * 100.0) / $requiredSeconds), 3)
    $expectedEndLocal = $processStartLocal.AddMinutes($StabilityWindowMinutes)
    $remainingSeconds = [Math]::Round([Math]::Max(0.0, ($expectedEndLocal - $now).TotalSeconds), 2)
} catch {
    $processRunning = $false
}

if ($processRunning) {
    if ($null -ne $latestIterationUpdatedLocal) {
        $minutesSinceLastIteration = [Math]::Round(($now - $latestIterationUpdatedLocal).TotalMinutes, 2)
        $stallDetected = ($minutesSinceLastIteration -gt $StabilityStallThresholdMinutes)
    } else {
        $minutesSinceLastIteration = [Math]::Round($elapsedSeconds / 60.0, 2)
        $stallDetected = ($minutesSinceLastIteration -gt $StabilityStallThresholdMinutes)
    }
}

$stabilitySummaryJson = Join-Path $StabilityWindowDir "stability-window-summary.json"
$stabilitySummaryExists = Test-Path $stabilitySummaryJson
$stabilityPass = $false
if ($stabilitySummaryExists) {
    $stabilitySummary = Get-Content -Path $stabilitySummaryJson -Raw | ConvertFrom-Json
    $stabilityPass = [bool]$stabilitySummary.pass
}

$auditIntakeExists = Test-Path $AuditIntakeRegister
$auditHandoffPackExists = Test-Path $AuditHandoffPackPath
$auditReportExists = Test-Path $AuditReportPath
$auditHandoffSha256 = ""
if ($auditHandoffPackExists) {
    $auditHandoffSha256 = (Get-FileHash -Path $AuditHandoffPackPath -Algorithm SHA256).Hash.ToLowerInvariant()
}

$auditReportParsed = $false
$auditCriticalCount = $null
$auditHighCount = $null
$auditMediumCount = $null
$auditLowCount = $null
$auditPolicyPass = $false

if ($auditReportExists) {
    $auditReportContent = Get-Content -Path $AuditReportPath -Raw
    $criticalMatch = [regex]::Match($auditReportContent, "(?im)^\s*critical_count\s*:\s*(\d+)\s*$")
    $highMatch = [regex]::Match($auditReportContent, "(?im)^\s*high_count\s*:\s*(\d+)\s*$")
    $mediumMatch = [regex]::Match($auditReportContent, "(?im)^\s*medium_count\s*:\s*(\d+)\s*$")
    $lowMatch = [regex]::Match($auditReportContent, "(?im)^\s*low_count\s*:\s*(\d+)\s*$")
    if ($criticalMatch.Success -and $highMatch.Success) {
        $auditCriticalCount = [int]$criticalMatch.Groups[1].Value
        $auditHighCount = [int]$highMatch.Groups[1].Value
        if ($mediumMatch.Success) {
            $auditMediumCount = [int]$mediumMatch.Groups[1].Value
        }
        if ($lowMatch.Success) {
            $auditLowCount = [int]$lowMatch.Groups[1].Value
        }
        $auditReportParsed = $true
        $auditPolicyPass = ($auditCriticalCount -eq 0 -and $auditHighCount -eq 0)
    }
}

$stabilityBlockerResolved = $stabilityPass
$auditBlockerResolved = ($auditReportExists -and $auditReportParsed -and $auditPolicyPass)
$allBlockersResolved = ($stabilityBlockerResolved -and $auditBlockerResolved)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    generated_at_local = $now.ToString("yyyy-MM-dd HH:mm:ss zzz")
    stability_window = [ordered]@{
        pid = $StabilityWindowPid
        process_running = $processRunning
        process_start_local = if ($null -ne $processStartLocal) { $processStartLocal.ToString("yyyy-MM-dd HH:mm:ss zzz") } else { "" }
        process_start_utc = if ($null -ne $processStartUtc) { $processStartUtc.ToString("o") } else { "" }
        expected_end_local = if ($null -ne $expectedEndLocal) { $expectedEndLocal.ToString("yyyy-MM-dd HH:mm:ss zzz") } else { "" }
        required_seconds = $requiredSeconds
        elapsed_seconds = $elapsedSeconds
        remaining_seconds = $remainingSeconds
        time_progress_pct = $timeProgressPct
        min_iterations = $StabilityMinIterations
        observed_iterations = $iterationCount
        iteration_progress_pct = $iterationProgressPct
        min_iterations_reached = $iterationMinReached
        latest_iteration_summary_json = $latestIterationSummaryJson
        latest_iteration_updated_local = if ($null -ne $latestIterationUpdatedLocal) { $latestIterationUpdatedLocal.ToString("yyyy-MM-dd HH:mm:ss zzz") } else { "" }
        minutes_since_last_iteration = $minutesSinceLastIteration
        stall_threshold_minutes = $StabilityStallThresholdMinutes
        stall_detected = $stallDetected
        summary_json = $stabilitySummaryJson
        summary_exists = $stabilitySummaryExists
        pass = $stabilityPass
        blocker_resolved = $stabilityBlockerResolved
    }
    third_party_audit = [ordered]@{
        intake_register = $AuditIntakeRegister
        intake_register_exists = $auditIntakeExists
        handoff_pack = $AuditHandoffPackPath
        handoff_pack_exists = $auditHandoffPackExists
        handoff_pack_sha256 = $auditHandoffSha256
        report_path = $AuditReportPath
        report_received = $auditReportExists
        report_parsed = $auditReportParsed
        critical_count = $auditCriticalCount
        high_count = $auditHighCount
        medium_count = $auditMediumCount
        low_count = $auditLowCount
        policy_pass = $auditPolicyPass
        blocker_resolved = $auditBlockerResolved
    }
    blockers = [ordered]@{
        stability_window_resolved = $stabilityBlockerResolved
        third_party_audit_resolved = $auditBlockerResolved
        all_resolved = $allBlockersResolved
    }
}

$summaryJson = Join-Path $OutputDir "week4-blocker-status.json"
$summaryMd = Join-Path $OutputDir "week4-blocker-status.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Week4 Blocker Status"
    ""
    "- generated_at_local: $($summary.generated_at_local)"
    "- all_blockers_resolved: $($summary.blockers.all_resolved)"
    ""
    "## Stability Window"
    ""
    "- process_running: $($summary.stability_window.process_running)"
    "- process_start_local: $($summary.stability_window.process_start_local)"
    "- expected_end_local: $($summary.stability_window.expected_end_local)"
    "- time_progress_pct: $($summary.stability_window.time_progress_pct)"
    "- observed_iterations: $($summary.stability_window.observed_iterations) / $($summary.stability_window.min_iterations)"
    "- iteration_progress_pct: $($summary.stability_window.iteration_progress_pct)"
    "- latest_iteration_updated_local: $($summary.stability_window.latest_iteration_updated_local)"
    "- minutes_since_last_iteration: $($summary.stability_window.minutes_since_last_iteration)"
    "- stall_threshold_minutes: $($summary.stability_window.stall_threshold_minutes)"
    "- stall_detected: $($summary.stability_window.stall_detected)"
    "- stability_pass: $($summary.stability_window.pass)"
    "- blocker_resolved: $($summary.stability_window.blocker_resolved)"
    "- summary_json: $($summary.stability_window.summary_json)"
    ""
    "## Third-Party Audit"
    ""
    "- intake_register_exists: $($summary.third_party_audit.intake_register_exists)"
    "- handoff_pack_exists: $($summary.third_party_audit.handoff_pack_exists)"
    "- handoff_pack_sha256: $($summary.third_party_audit.handoff_pack_sha256)"
    "- report_received: $($summary.third_party_audit.report_received)"
    "- report_parsed: $($summary.third_party_audit.report_parsed)"
    "- critical_count: $($summary.third_party_audit.critical_count)"
    "- high_count: $($summary.third_party_audit.high_count)"
    "- medium_count: $($summary.third_party_audit.medium_count)"
    "- low_count: $($summary.third_party_audit.low_count)"
    "- policy_pass: $($summary.third_party_audit.policy_pass)"
    "- blocker_resolved: $($summary.third_party_audit.blocker_resolved)"
    "- intake_register: $($summary.third_party_audit.intake_register)"
    "- report_path: $($summary.third_party_audit.report_path)"
)

$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "week4 blocker status generated:"
Write-Host "  all_blockers_resolved: $($summary.blockers.all_resolved)"
Write-Host "  stability_process_running: $($summary.stability_window.process_running)"
Write-Host "  stability_expected_end_local: $($summary.stability_window.expected_end_local)"
Write-Host "  stability_time_progress_pct: $($summary.stability_window.time_progress_pct)"
Write-Host "  stability_iteration_progress_pct: $($summary.stability_window.iteration_progress_pct)"
Write-Host "  stability_minutes_since_last_iteration: $($summary.stability_window.minutes_since_last_iteration)"
Write-Host "  stability_stall_detected: $($summary.stability_window.stall_detected)"
Write-Host "  audit_report_received: $($summary.third_party_audit.report_received)"
Write-Host "  audit_report_parsed: $($summary.third_party_audit.report_parsed)"
Write-Host "  audit_critical_count: $($summary.third_party_audit.critical_count)"
Write-Host "  audit_high_count: $($summary.third_party_audit.high_count)"
Write-Host "  audit_policy_pass: $($summary.third_party_audit.policy_pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"
