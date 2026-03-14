param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(0, 10080)]
    [int]$WindowMinutes = 4320,
    [ValidateRange(1, 1440)]
    [int]$IterationIntervalMinutes = 60,
    [ValidateRange(1, 5000)]
    [int]$MinIterations = 1,
    [ValidateRange(0, 5000)]
    [int]$MaxIterations = 0,
    [ValidateRange(2, 20)]
    [int]$AdapterRuns = 3,
    [ValidateSet("core", "persist", "wasm")]
    [string]$CapabilityVariant = "core",
    [bool]$FailFast = $true,
    [switch]$NoSleep
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\stability-window-gate"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$adapterStabilityScript = Join-Path $RepoRoot "scripts\migration\run_adapter_stability_gate.ps1"
if (-not (Test-Path $adapterStabilityScript)) {
    throw "missing adapter stability gate script: $adapterStabilityScript"
}

function Push-ScopedEnvClear {
    param([string[]]$Names)
    $state = @{}
    foreach ($name in $Names) {
        $path = "Env:$name"
        if (Test-Path $path) {
            $state[$name] = (Get-Item $path).Value
            Remove-Item $path
        } else {
            $state[$name] = $null
        }
    }
    return $state
}

function Pop-ScopedEnvClear {
    param([hashtable]$State)
    foreach ($entry in $State.GetEnumerator()) {
        $path = "Env:$($entry.Key)"
        if ($null -eq $entry.Value) {
            Remove-Item $path -ErrorAction SilentlyContinue
        } else {
            Set-Item $path -Value $entry.Value
        }
    }
}

$windowStart = Get-Date
$deadline = $windowStart.AddMinutes($WindowMinutes)
$runReports = @()
$allRunsPass = $true
$stopReason = "deadline_reached"

while ($true) {
    $now = Get-Date
    $meetsWindow = ($now -ge $deadline)
    $meetsMinIterations = ($runReports.Count -ge $MinIterations)
    if ($meetsWindow -and $meetsMinIterations) {
        $stopReason = "deadline_reached"
        break
    }
    if ($MaxIterations -gt 0 -and $runReports.Count -ge $MaxIterations) {
        $stopReason = "max_iterations_reached"
        break
    }

    $iteration = $runReports.Count + 1
    $iterationDir = Join-Path $OutputDir ("iteration-" + $iteration.ToString("D4"))
    New-Item -ItemType Directory -Force -Path $iterationDir | Out-Null

    $iterationStart = Get-Date
    $iterationPass = $false
    $iterationError = ""
    $summaryPath = Join-Path $iterationDir "adapter-stability-summary.json"

    try {
        $envState = Push-ScopedEnvClear -Names @(
            "NOVOVM_TX_WIRE_FILE",
            "NOVOVM_OPS_WIRE_FILE",
            "NOVOVM_OPS_WIRE_DIR"
        )
        try {
            & $adapterStabilityScript `
                -RepoRoot $RepoRoot `
                -OutputDir $iterationDir `
                -Runs $AdapterRuns `
                -CapabilityVariant $CapabilityVariant | Out-Null
        } finally {
            Pop-ScopedEnvClear -State $envState
        }

        if (-not (Test-Path $summaryPath)) {
            throw "missing adapter stability summary json: $summaryPath"
        }
        $iterationSummary = Get-Content -Path $summaryPath -Raw | ConvertFrom-Json
        $iterationPass = [bool]$iterationSummary.pass
    } catch {
        $iterationPass = $false
        $iterationError = $_.Exception.Message
    }

    $iterationEnd = Get-Date
    $durationSec = [Math]::Round(($iterationEnd - $iterationStart).TotalSeconds, 2)
    $runReports += [ordered]@{
        iteration = $iteration
        started_at_utc = $iterationStart.ToUniversalTime().ToString("o")
        finished_at_utc = $iterationEnd.ToUniversalTime().ToString("o")
        duration_seconds = $durationSec
        pass = $iterationPass
        error = $iterationError
        adapter_runs = $AdapterRuns
        summary_json = $summaryPath
        iteration_dir = $iterationDir
    }

    if (-not $iterationPass) {
        $allRunsPass = $false
        if ($FailFast) {
            $stopReason = "failed_iteration"
            break
        }
    }

    $nextTarget = $iterationStart.AddMinutes($IterationIntervalMinutes)
    if (-not $NoSleep) {
        $sleepSeconds = [Math]::Ceiling(($nextTarget - (Get-Date)).TotalSeconds)
        if ($sleepSeconds -gt 0) {
            Start-Sleep -Seconds $sleepSeconds
        }
    }
}

$windowEnd = Get-Date
$elapsedSeconds = [Math]::Round(($windowEnd - $windowStart).TotalSeconds, 2)
$requiredSeconds = [Math]::Round($WindowMinutes * 60.0, 2)
$windowReached = ($elapsedSeconds -ge $requiredSeconds)
$minIterationsReached = ($runReports.Count -ge $MinIterations)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    started_at_utc = $windowStart.ToUniversalTime().ToString("o")
    finished_at_utc = $windowEnd.ToUniversalTime().ToString("o")
    requested_window_minutes = $WindowMinutes
    elapsed_seconds = $elapsedSeconds
    required_seconds = $requiredSeconds
    iteration_interval_minutes = $IterationIntervalMinutes
    min_iterations = $MinIterations
    max_iterations = $MaxIterations
    adapter_runs = $AdapterRuns
    capability_variant = $CapabilityVariant
    fail_fast = $FailFast
    no_sleep = [bool]$NoSleep
    run_count = $runReports.Count
    all_runs_pass = $allRunsPass
    window_reached = $windowReached
    min_iterations_reached = $minIterationsReached
    stop_reason = $stopReason
    pass = ($allRunsPass -and $windowReached -and $minIterationsReached)
    run_reports = $runReports
}

$summaryJson = Join-Path $OutputDir "stability-window-summary.json"
$summaryMd = Join-Path $OutputDir "stability-window-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Stability Window Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- started_at_utc: $($summary.started_at_utc)"
    "- finished_at_utc: $($summary.finished_at_utc)"
    "- requested_window_minutes: $($summary.requested_window_minutes)"
    "- elapsed_seconds: $($summary.elapsed_seconds)"
    "- iteration_interval_minutes: $($summary.iteration_interval_minutes)"
    "- min_iterations: $($summary.min_iterations)"
    "- max_iterations: $($summary.max_iterations)"
    "- adapter_runs: $($summary.adapter_runs)"
    "- capability_variant: $($summary.capability_variant)"
    "- run_count: $($summary.run_count)"
    "- all_runs_pass: $($summary.all_runs_pass)"
    "- window_reached: $($summary.window_reached)"
    "- stop_reason: $($summary.stop_reason)"
    "- pass: $($summary.pass)"
    ""
    "## Run Reports"
    ""
    "| iteration | pass | duration_seconds | summary_json |"
    "|---|---|---|---|"
)
foreach ($r in $runReports) {
    $md += "| $($r.iteration) | $($r.pass) | $($r.duration_seconds) | $($r.summary_json) |"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "stability window gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  run_count: $($summary.run_count)"
Write-Host "  window_reached: $($summary.window_reached)"
Write-Host "  stop_reason: $($summary.stop_reason)"
Write-Host "  summary_json: $summaryJson"

if (-not $summary.pass) {
    throw "stability window gate FAILED (all_runs_pass=$allRunsPass, window_reached=$windowReached, min_iterations_reached=$minIterationsReached, stop_reason=$stopReason)"
}

Write-Host "stability window gate PASS"
