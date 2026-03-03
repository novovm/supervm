param(
    [string]$RepoRoot = "D:\WorksArea\SUPERVM",
    [string]$OutputDir = "D:\WorksArea\SUPERVM\artifacts\migration\acceptance-gate",
    [double]$AllowedRegressionPct = -5.0,
    [ValidateRange(1, 9)]
    [int]$PerformanceRuns = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Require-Path {
    param([string]$Path, [string]$Name)
    if (-not (Test-Path $Path)) {
        throw "missing ${Name}: $Path"
    }
}

$functionalScript = Join-Path $RepoRoot "scripts\migration\run_functional_consistency.ps1"
$performanceGateScript = Join-Path $RepoRoot "scripts\migration\run_performance_gate_seal_single.ps1"
Require-Path -Path $functionalScript -Name "functional script"
Require-Path -Path $performanceGateScript -Name "performance gate script"

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$functionalOutputDir = Join-Path $OutputDir "functional"
$performanceOutputDir = Join-Path $OutputDir "performance-gate"
New-Item -ItemType Directory -Force -Path $functionalOutputDir | Out-Null
New-Item -ItemType Directory -Force -Path $performanceOutputDir | Out-Null

Write-Host "acceptance gate: functional consistency ..."
& $functionalScript -RepoRoot $RepoRoot -OutputDir $functionalOutputDir | Out-Null

Write-Host "acceptance gate: performance seal gate ..."
& $performanceGateScript `
    -RepoRoot $RepoRoot `
    -OutputDir $performanceOutputDir `
    -AllowedRegressionPct $AllowedRegressionPct `
    -Runs $PerformanceRuns | Out-Null

$functionalJson = Join-Path $functionalOutputDir "functional-consistency.json"
$performanceJson = Join-Path $performanceOutputDir "performance-gate-summary.json"
Require-Path -Path $functionalJson -Name "functional report json"
Require-Path -Path $performanceJson -Name "performance gate summary json"

$functional = Get-Content -Path $functionalJson -Raw | ConvertFrom-Json
$performance = Get-Content -Path $performanceJson -Raw | ConvertFrom-Json

$functionalPass = [bool]$functional.overall_pass
$performancePass = [bool]$performance.pass
$overallPass = ($functionalPass -and $performancePass)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    overall_pass = $overallPass
    functional_pass = $functionalPass
    performance_pass = $performancePass
    functional_report_json = $functionalJson
    performance_report_json = $performanceJson
    performance_runs = $PerformanceRuns
    allowed_regression_pct = $AllowedRegressionPct
}

$summaryJson = Join-Path $OutputDir "acceptance-gate-summary.json"
$summaryMd = Join-Path $OutputDir "acceptance-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Migration Acceptance Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- overall_pass: $($summary.overall_pass)"
    "- functional_pass: $($summary.functional_pass)"
    "- performance_pass: $($summary.performance_pass)"
    "- performance_runs: $($summary.performance_runs)"
    "- allowed_regression_pct: $($summary.allowed_regression_pct)"
    "- functional_report_json: $($summary.functional_report_json)"
    "- performance_report_json: $($summary.performance_report_json)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "acceptance gate summary:"
Write-Host "  overall_pass: $overallPass"
Write-Host "  functional_report: $functionalJson"
Write-Host "  performance_report: $performanceJson"
Write-Host "  summary_json: $summaryJson"

if (-not $overallPass) {
    throw "migration acceptance gate FAILED (functional_pass=$functionalPass, performance_pass=$performancePass)"
}

Write-Host "migration acceptance gate PASS"
