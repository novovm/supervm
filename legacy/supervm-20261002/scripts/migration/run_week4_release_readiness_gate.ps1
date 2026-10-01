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
    [string]$AuditReportPath = "",
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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-release-readiness-gate"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$blockerScript = Join-Path $RepoRoot "scripts\migration\run_week4_blocker_status.ps1"
if (-not (Test-Path $blockerScript)) {
    throw "missing blocker status script: $blockerScript"
}

& $blockerScript `
    -RepoRoot $RepoRoot `
    -OutputDir (Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-blocker-status") `
    -StabilityWindowPid $StabilityWindowPid `
    -StabilityWindowDir $StabilityWindowDir `
    -StabilityWindowMinutes $StabilityWindowMinutes `
    -StabilityMinIterations $StabilityMinIterations `
    -StabilityStallThresholdMinutes $StabilityStallThresholdMinutes `
    -AuditIntakeRegister $AuditIntakeRegister `
    -AuditHandoffPackPath $AuditHandoffPackPath `
    -AuditReportPath $AuditReportPath | Out-Null

$blockerStatusJson = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-blocker-status\week4-blocker-status.json"
if (-not (Test-Path $blockerStatusJson)) {
    throw "missing blocker status json: $blockerStatusJson"
}
$status = Get-Content -Path $blockerStatusJson -Raw | ConvertFrom-Json

$reasons = @()
if (-not [bool]$status.blockers.stability_window_resolved) {
    $reasons += "stability_window_not_resolved"
    if ([bool]$status.stability_window.process_running) {
        $reasons += "stability_window_running_until_$($status.stability_window.expected_end_local)"
    } else {
        $reasons += "stability_window_process_not_running"
    }
    if ([bool]$status.stability_window.stall_detected) {
        $reasons += "stability_window_stall_detected"
    }
}
if (-not [bool]$status.blockers.third_party_audit_resolved) {
    $reasons += "third_party_audit_not_resolved"
    if (-not [bool]$status.third_party_audit.report_received) {
        $reasons += "audit_report_not_received"
    } elseif (-not [bool]$status.third_party_audit.report_parsed) {
        $reasons += "audit_report_not_machine_parseable"
    } elseif (-not [bool]$status.third_party_audit.policy_pass) {
        $reasons += "audit_policy_not_passed_critical_or_high_nonzero"
    }
}

$pass = [bool]$status.blockers.all_resolved
$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    profile = "week4_release_readiness_gate_v1"
    pass = $pass
    decision = if ($pass) { "GO" } else { "NO-GO" }
    blocker_status_json = $blockerStatusJson
    reasons = $reasons
    stability_window = [ordered]@{
        resolved = [bool]$status.blockers.stability_window_resolved
        process_running = [bool]$status.stability_window.process_running
        expected_end_local = [string]$status.stability_window.expected_end_local
        time_progress_pct = [double]$status.stability_window.time_progress_pct
        iteration_progress_pct = [double]$status.stability_window.iteration_progress_pct
        observed_iterations = [int]$status.stability_window.observed_iterations
        min_iterations = [int]$status.stability_window.min_iterations
        minutes_since_last_iteration = $status.stability_window.minutes_since_last_iteration
        stall_detected = [bool]$status.stability_window.stall_detected
        pass = [bool]$status.stability_window.pass
    }
    third_party_audit = [ordered]@{
        resolved = [bool]$status.blockers.third_party_audit_resolved
        report_received = [bool]$status.third_party_audit.report_received
        report_parsed = [bool]$status.third_party_audit.report_parsed
        critical_count = $status.third_party_audit.critical_count
        high_count = $status.third_party_audit.high_count
        medium_count = $status.third_party_audit.medium_count
        low_count = $status.third_party_audit.low_count
        policy_pass = [bool]$status.third_party_audit.policy_pass
        report_path = [string]$status.third_party_audit.report_path
    }
}

$summaryJson = Join-Path $OutputDir "week4-release-readiness-summary.json"
$summaryMd = Join-Path $OutputDir "week4-release-readiness-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Week4 Release Readiness Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- profile: $($summary.profile)"
    "- decision: $($summary.decision)"
    "- pass: $($summary.pass)"
    "- blocker_status_json: $($summary.blocker_status_json)"
    "- reasons: $([string]::Join(', ', $summary.reasons))"
    ""
    "## Stability Window"
    ""
    "- resolved: $($summary.stability_window.resolved)"
    "- process_running: $($summary.stability_window.process_running)"
    "- expected_end_local: $($summary.stability_window.expected_end_local)"
    "- time_progress_pct: $($summary.stability_window.time_progress_pct)"
    "- iteration_progress_pct: $($summary.stability_window.iteration_progress_pct)"
    "- observed_iterations: $($summary.stability_window.observed_iterations) / $($summary.stability_window.min_iterations)"
    "- minutes_since_last_iteration: $($summary.stability_window.minutes_since_last_iteration)"
    "- stall_detected: $($summary.stability_window.stall_detected)"
    ""
    "## Third-Party Audit"
    ""
    "- resolved: $($summary.third_party_audit.resolved)"
    "- report_received: $($summary.third_party_audit.report_received)"
    "- report_parsed: $($summary.third_party_audit.report_parsed)"
    "- critical_count: $($summary.third_party_audit.critical_count)"
    "- high_count: $($summary.third_party_audit.high_count)"
    "- medium_count: $($summary.third_party_audit.medium_count)"
    "- low_count: $($summary.third_party_audit.low_count)"
    "- policy_pass: $($summary.third_party_audit.policy_pass)"
    "- report_path: $($summary.third_party_audit.report_path)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "week4 release readiness gate summary:"
Write-Host "  decision: $($summary.decision)"
Write-Host "  pass: $($summary.pass)"
Write-Host "  reasons: $([string]::Join(', ', $summary.reasons))"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"

if (-not $summary.pass -and -not $NoThrow) {
    throw "week4 release readiness gate FAILED: $([string]::Join(', ', $summary.reasons))"
}

if ($summary.pass) {
    Write-Host "week4 release readiness gate PASS"
} else {
    Write-Host "week4 release readiness gate NOT READY (NoThrow=$NoThrow)"
}
