param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 60)]
    [int]$MaxWatchdogInstances = 1,
    [ValidateRange(1, 240)]
    [int]$MaxSummaryAgeMinutes = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-automation-healthcheck"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$now = Get-Date
$issues = @()
$warnings = @()

$requiredScripts = @(
    "scripts\migration\run_week4_blocker_status.ps1",
    "scripts\migration\run_week4_release_readiness_gate.ps1",
    "scripts\migration\run_week4_closeout.ps1",
    "scripts\migration\run_week4_watchdog.ps1",
    "scripts\migration\ingest_third_party_audit_report.ps1"
)

$scriptChecks = @()
foreach ($relative in $requiredScripts) {
    $path = Join-Path $RepoRoot $relative
    $exists = Test-Path $path
    $scriptChecks += [ordered]@{
        script = $relative
        path = $path
        exists = $exists
    }
    if (-not $exists) {
        $issues += "missing_script:$relative"
    }
}

$watchdogMatches = @()
try {
    $psOutput = & ps -eo pid,cmd
    $watchdogMatches = @(
        $psOutput |
            Select-Object -Skip 1 |
            Where-Object { $_ -match "run_week4_watchdog\.ps1" } |
            ForEach-Object {
                $m = [regex]::Match($_, "^\s*(\d+)\s+(.+)$")
                if ($m.Success) {
                    [ordered]@{
                        pid = [int]$m.Groups[1].Value
                        cmd = $m.Groups[2].Value
                    }
                }
            }
    )
} catch {
    $issues += "watchdog_process_scan_failed:$($_.Exception.Message)"
}

$watchdogCount = $watchdogMatches.Count
if ($watchdogCount -eq 0) {
    $issues += "watchdog_not_running"
} elseif ($watchdogCount -gt $MaxWatchdogInstances) {
    $issues += "watchdog_instance_count_exceeded:$watchdogCount"
}

function Parse-UtcString {
    param([string]$Value)
    if (-not $Value) { return $null }
    try {
        return [DateTime]::Parse($Value, $null, [System.Globalization.DateTimeStyles]::RoundtripKind)
    } catch {
        return $null
    }
}

function Check-SummaryFreshness {
    param(
        [string]$Path,
        [string]$Label,
        [int]$MaxAgeMinutes,
        [ref]$IssuesRef,
        [ref]$WarningsRef
    )

    $result = [ordered]@{
        label = $Label
        path = $Path
        exists = $false
        generated_at_utc = ""
        age_minutes = $null
        fresh = $false
        parsed = $false
    }

    if (-not (Test-Path $Path)) {
        $IssuesRef.Value += "$Label`_summary_missing"
        return $result
    }

    $result.exists = $true
    try {
        $json = Get-Content -Path $Path -Raw | ConvertFrom-Json
        $generatedAt = Parse-UtcString -Value ([string]$json.generated_at_utc)
        if ($null -eq $generatedAt) {
            $WarningsRef.Value += "$Label`_generated_at_invalid"
            return $result
        }
        $result.generated_at_utc = $generatedAt.ToUniversalTime().ToString("o")
        $age = ($now.ToUniversalTime() - $generatedAt.ToUniversalTime()).TotalMinutes
        $result.age_minutes = [Math]::Round($age, 2)
        $result.parsed = $true
        $result.fresh = ($age -le $MaxAgeMinutes)
        if (-not $result.fresh) {
            $IssuesRef.Value += "$Label`_summary_stale:$([Math]::Round($age,2))"
        }
    } catch {
        $IssuesRef.Value += "$Label`_summary_parse_failed:$($_.Exception.Message)"
    }

    return $result
}

$blockerPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-blocker-status\week4-blocker-status.json"
$readinessPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-release-readiness-gate\week4-release-readiness-summary.json"
$watchdogSummaryPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-watchdog\week4-watchdog-summary.json"
$closeoutPath = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-closeout\week4-closeout-summary.json"

$blockerFreshness = Check-SummaryFreshness -Path $blockerPath -Label "blocker" -MaxAgeMinutes $MaxSummaryAgeMinutes -IssuesRef ([ref]$issues) -WarningsRef ([ref]$warnings)
$readinessFreshness = Check-SummaryFreshness -Path $readinessPath -Label "readiness" -MaxAgeMinutes $MaxSummaryAgeMinutes -IssuesRef ([ref]$issues) -WarningsRef ([ref]$warnings)
$watchdogFreshness = Check-SummaryFreshness -Path $watchdogSummaryPath -Label "watchdog" -MaxAgeMinutes $MaxSummaryAgeMinutes -IssuesRef ([ref]$issues) -WarningsRef ([ref]$warnings)
$closeoutFreshness = Check-SummaryFreshness -Path $closeoutPath -Label "closeout" -MaxAgeMinutes ($MaxSummaryAgeMinutes * 2) -IssuesRef ([ref]$issues) -WarningsRef ([ref]$warnings)

$stabilityProcessRunning = $false
$stabilityStallDetected = $false
$stabilityPid = $null
if (Test-Path $blockerPath) {
    try {
        $blocker = Get-Content -Path $blockerPath -Raw | ConvertFrom-Json
        $stabilityPid = [int]$blocker.stability_window.pid
        $stabilityStallDetected = [bool]$blocker.stability_window.stall_detected
        if ($stabilityStallDetected) {
            $issues += "stability_stall_detected"
        }
        try {
            Get-Process -Id $stabilityPid -ErrorAction Stop | Out-Null
            $stabilityProcessRunning = $true
        } catch {
            $stabilityProcessRunning = $false
            if (-not [bool]$blocker.stability_window.pass) {
                $issues += "stability_process_not_running:$stabilityPid"
            }
        }
    } catch {
        $warnings += "blocker_details_parse_failed:$($_.Exception.Message)"
    }
}

$pass = ($issues.Count -eq 0)
$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    generated_at_local = $now.ToString("yyyy-MM-dd HH:mm:ss zzz")
    profile = "week4_automation_healthcheck_v1"
    pass = $pass
    max_watchdog_instances = $MaxWatchdogInstances
    max_summary_age_minutes = $MaxSummaryAgeMinutes
    issues = $issues
    warnings = $warnings
    scripts = $scriptChecks
    watchdog = [ordered]@{
        process_count = $watchdogCount
        processes = $watchdogMatches
        summary = $watchdogFreshness
    }
    blocker = [ordered]@{
        summary = $blockerFreshness
        stability_pid = $stabilityPid
        stability_process_running = $stabilityProcessRunning
        stability_stall_detected = $stabilityStallDetected
    }
    readiness = [ordered]@{
        summary = $readinessFreshness
    }
    closeout = [ordered]@{
        summary = $closeoutFreshness
    }
}

$summaryJson = Join-Path $OutputDir "week4-automation-healthcheck-summary.json"
$summaryMd = Join-Path $OutputDir "week4-automation-healthcheck-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Week4 Automation Healthcheck Summary"
    ""
    "- generated_at_local: $($summary.generated_at_local)"
    "- profile: $($summary.profile)"
    "- pass: $($summary.pass)"
    "- issues: $([string]::Join(', ', @($summary.issues)))"
    "- warnings: $([string]::Join(', ', @($summary.warnings)))"
    ""
    "## Watchdog"
    ""
    "- process_count: $($summary.watchdog.process_count)"
    "- summary_fresh: $($summary.watchdog.summary.fresh)"
    "- summary_age_minutes: $($summary.watchdog.summary.age_minutes)"
    ""
    "## Blocker"
    ""
    "- summary_fresh: $($summary.blocker.summary.fresh)"
    "- summary_age_minutes: $($summary.blocker.summary.age_minutes)"
    "- stability_pid: $($summary.blocker.stability_pid)"
    "- stability_process_running: $($summary.blocker.stability_process_running)"
    "- stability_stall_detected: $($summary.blocker.stability_stall_detected)"
    ""
    "## Readiness"
    ""
    "- summary_fresh: $($summary.readiness.summary.fresh)"
    "- summary_age_minutes: $($summary.readiness.summary.age_minutes)"
    ""
    "## Closeout"
    ""
    "- summary_exists: $($summary.closeout.summary.exists)"
    "- summary_fresh: $($summary.closeout.summary.fresh)"
    "- summary_age_minutes: $($summary.closeout.summary.age_minutes)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "week4 automation healthcheck summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  issues: $([string]::Join(', ', @($summary.issues)))"
Write-Host "  warnings: $([string]::Join(', ', @($summary.warnings)))"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"
