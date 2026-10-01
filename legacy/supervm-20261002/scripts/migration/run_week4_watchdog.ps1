param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1, 1440)]
    [int]$IntervalMinutes = 10,
    [ValidateRange(1, 10080)]
    [int]$DurationMinutes = 4320,
    [switch]$SingleRun,
    [switch]$NoSleep,
    [bool]$StopOnGo = $true,
    [bool]$PromoteOnGo = $true
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-watchdog"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$readinessScript = Join-Path $RepoRoot "scripts\migration\run_week4_release_readiness_gate.ps1"
if (-not (Test-Path $readinessScript)) {
    throw "missing readiness gate script: $readinessScript"
}
$promotionScript = Join-Path $RepoRoot "scripts\migration\run_week4_closeout.ps1"
if ($PromoteOnGo -and -not (Test-Path $promotionScript)) {
    throw "missing week4 closeout script: $promotionScript"
}

$readinessJson = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-release-readiness-gate\week4-release-readiness-summary.json"
$readinessMd = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-release-readiness-gate\week4-release-readiness-summary.md"
$promotionSummaryJson = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-closeout\week4-closeout-summary.json"
$promotionSummaryMd = Join-Path $RepoRoot "artifacts\migration\week1-2026-03-13\week4-closeout\week4-closeout-summary.md"

$start = Get-Date
$deadline = $start.AddMinutes($DurationMinutes)
$ticks = @()
$stopReason = "duration_elapsed"

$iteration = 0
while ($true) {
    if ($iteration -gt 0 -and -not $SingleRun) {
        if ((Get-Date) -ge $deadline) {
            $stopReason = "duration_elapsed"
            break
        }
    }

    $iteration += 1
    $tickStart = Get-Date
    $tickDecision = "NO-GO"
    $tickPass = $false
    $tickReasons = @("readiness_summary_missing")
    $tickError = ""
    $tickPromotionAttempted = $false
    $tickPromotionSucceeded = $false
    $tickPromotionError = ""
    $shouldStopThisTick = $false

    try {
        & $readinessScript -RepoRoot $RepoRoot -NoThrow | Out-Null
        if (Test-Path $readinessJson) {
            $r = Get-Content -Path $readinessJson -Raw | ConvertFrom-Json
            $tickDecision = [string]$r.decision
            $tickPass = [bool]$r.pass
            $tickReasons = @($r.reasons | ForEach-Object { [string]$_ })
            if ($tickReasons.Count -eq 0) {
                $tickReasons = @()
            }
        }
    } catch {
        $tickError = $_.Exception.Message
        $tickDecision = "NO-GO"
        $tickPass = $false
        $tickReasons = @("watchdog_readiness_exception")
    }

    if ($tickPass -and $StopOnGo) {
        if ($PromoteOnGo) {
            $tickPromotionAttempted = $true
            try {
                & $promotionScript -RepoRoot $RepoRoot -NoThrow -NoRefreshReadinessGate | Out-Null
                if (Test-Path $promotionSummaryJson) {
                    $promotionSummary = Get-Content -Path $promotionSummaryJson -Raw | ConvertFrom-Json
                    if ($null -ne $promotionSummary.closed_out) {
                        $tickPromotionSucceeded = [bool]$promotionSummary.closed_out
                    } else {
                        $tickPromotionSucceeded = [bool]$promotionSummary.promoted
                    }
                    if (-not $tickPromotionSucceeded) {
                        $tickPromotionError = "promotion_not_executed"
                    }
                } else {
                    $tickPromotionError = "promotion_summary_missing"
                }
            } catch {
                $tickPromotionError = $_.Exception.Message
            }

            if ($tickPromotionSucceeded) {
                $stopReason = "go_reached_closeout_completed"
                $shouldStopThisTick = $true
            } else {
                $stopReason = "go_reached_closeout_pending"
            }
        } else {
            $stopReason = "go_reached"
            $shouldStopThisTick = $true
        }
    }

    $tickEnd = Get-Date
    $tickDurationSec = [Math]::Round(($tickEnd - $tickStart).TotalSeconds, 2)
    $ticks += [ordered]@{
        iteration = $iteration
        tick_started_at_utc = $tickStart.ToUniversalTime().ToString("o")
        tick_finished_at_utc = $tickEnd.ToUniversalTime().ToString("o")
        tick_duration_seconds = $tickDurationSec
        decision = $tickDecision
        pass = $tickPass
        reasons = $tickReasons
        error = $tickError
        readiness_json = $readinessJson
        readiness_md = $readinessMd
        promotion_attempted = $tickPromotionAttempted
        promotion_succeeded = $tickPromotionSucceeded
        promotion_error = $tickPromotionError
        promotion_summary_json = $promotionSummaryJson
        promotion_summary_md = $promotionSummaryMd
    }

    if ($shouldStopThisTick) {
        break
    }

    if ($SingleRun) {
        $stopReason = "single_run"
        break
    }

    if (-not $NoSleep) {
        $nextTick = $tickStart.AddMinutes($IntervalMinutes)
        $sleepSec = [Math]::Ceiling(($nextTick - (Get-Date)).TotalSeconds)
        if ($sleepSec -gt 0) {
            Start-Sleep -Seconds $sleepSec
        }
    }
}

$end = Get-Date
$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    started_at_utc = $start.ToUniversalTime().ToString("o")
    finished_at_utc = $end.ToUniversalTime().ToString("o")
    interval_minutes = $IntervalMinutes
    duration_minutes = $DurationMinutes
    single_run = [bool]$SingleRun
    no_sleep = [bool]$NoSleep
    stop_on_go = [bool]$StopOnGo
    promote_on_go = [bool]$PromoteOnGo
    stop_reason = $stopReason
    tick_count = $ticks.Count
    last_decision = if ($ticks.Count -gt 0) { [string]$ticks[-1].decision } else { "" }
    last_pass = if ($ticks.Count -gt 0) { [bool]$ticks[-1].pass } else { $false }
    last_promotion_attempted = if ($ticks.Count -gt 0) { [bool]$ticks[-1].promotion_attempted } else { $false }
    last_promotion_succeeded = if ($ticks.Count -gt 0) { [bool]$ticks[-1].promotion_succeeded } else { $false }
    ticks = $ticks
}

$summaryJson = Join-Path $OutputDir "week4-watchdog-summary.json"
$summaryMd = Join-Path $OutputDir "week4-watchdog-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Week4 Watchdog Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- started_at_utc: $($summary.started_at_utc)"
    "- finished_at_utc: $($summary.finished_at_utc)"
    "- interval_minutes: $($summary.interval_minutes)"
    "- duration_minutes: $($summary.duration_minutes)"
    "- single_run: $($summary.single_run)"
    "- stop_on_go: $($summary.stop_on_go)"
    "- promote_on_go: $($summary.promote_on_go)"
    "- stop_reason: $($summary.stop_reason)"
    "- tick_count: $($summary.tick_count)"
    "- last_decision: $($summary.last_decision)"
    "- last_pass: $($summary.last_pass)"
    "- last_promotion_attempted: $($summary.last_promotion_attempted)"
    "- last_promotion_succeeded: $($summary.last_promotion_succeeded)"
    ""
    "## Ticks"
    ""
    "| iteration | decision | pass | reasons | error | promotion_attempted | promotion_succeeded | promotion_error |"
    "|---|---|---|---|---|---|---|---|"
)

foreach ($t in $ticks) {
    $reasonText = [string]::Join(";", @($t.reasons))
    $md += "| $($t.iteration) | $($t.decision) | $($t.pass) | $reasonText | $($t.error) | $($t.promotion_attempted) | $($t.promotion_succeeded) | $($t.promotion_error) |"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "week4 watchdog summary:"
Write-Host "  stop_reason: $($summary.stop_reason)"
Write-Host "  tick_count: $($summary.tick_count)"
Write-Host "  last_decision: $($summary.last_decision)"
Write-Host "  last_pass: $($summary.last_pass)"
Write-Host "  last_promotion_attempted: $($summary.last_promotion_attempted)"
Write-Host "  last_promotion_succeeded: $($summary.last_promotion_succeeded)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md: $summaryMd"
