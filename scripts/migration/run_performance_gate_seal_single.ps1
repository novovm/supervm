param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [double]$AllowedRegressionPct = -5.0,
    [ValidateRange(1, 9)]
    [int]$Runs = 3,
    [ValidateSet("core", "persist", "wasm")]
    [string]$Variant = "core",
    [string]$BaselineJson = "",
    [bool]$IncludeCapabilitySnapshot = $true,
    [ValidateSet("core", "persist", "wasm")]
    [string]$CapabilityVariant = "core",
    [string]$CapabilityJson = "",
    [string]$AoemPluginDir = "",
    [bool]$PreferComposedAoemRuntime = $true
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\performance-gate\seal-single"
}

function Get-Median {
    param([double[]]$Values)
    if (-not $Values -or $Values.Count -eq 0) {
        throw "median input cannot be empty"
    }
    $sorted = @($Values | Sort-Object)
    $n = $sorted.Count
    $mid = [int]($n / 2)
    if (($n % 2) -eq 1) {
        return [double]$sorted[$mid]
    }
    return (([double]$sorted[$mid - 1] + [double]$sorted[$mid]) / 2.0)
}

if ($Variant -ne "core") {
    throw "seal_single gate baseline currently only supports Variant=core"
}

$compareScript = Join-Path $RepoRoot "scripts\migration\run_performance_compare.ps1"
if (-not (Test-Path $compareScript)) {
    throw "missing compare script: $compareScript"
}

$baselinePath = $BaselineJson
if (-not $baselinePath) {
    $baselinePath = Join-Path $RepoRoot "scripts\migration\baselines\aoem-seal-core-2026-03-02.json"
}
if (-not (Test-Path $baselinePath)) {
    throw "missing seal baseline json: $baselinePath"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$runDirs = @()
$runReports = @()
$baselineByCase = @{}
$samplesByCase = @{}

$compareParams = @{
    RepoRoot = $RepoRoot
    OutputDir = ""
    BaselineJson = $baselinePath
    Variants = $Variant
    AllowedRegressionPct = $AllowedRegressionPct
    BuildProfile = "release"
    LineProfile = "seal_single"
    WarmupCalls = 5
    IncludeCapabilitySnapshot = $IncludeCapabilitySnapshot
    CapabilityVariant = $CapabilityVariant
}
if ($CapabilityJson) {
    $compareParams["CapabilityJson"] = $CapabilityJson
}
if ($AoemPluginDir) {
    $compareParams["AoemPluginDir"] = $AoemPluginDir
}
$compareParams["PreferComposedAoemRuntime"] = $PreferComposedAoemRuntime

for ($run = 1; $run -le $Runs; $run++) {
    $runDir = Join-Path $OutputDir ("run-" + $run)
    New-Item -ItemType Directory -Force -Path $runDir | Out-Null
    $runDirs += $runDir
    $compareParams["OutputDir"] = $runDir

    & $compareScript @compareParams | Out-Null

    $reportJson = Join-Path $runDir "performance-compare.json"
    if (-not (Test-Path $reportJson)) {
        throw "performance gate report missing: $reportJson"
    }

    $report = Get-Content -Path $reportJson -Raw | ConvertFrom-Json
    if (-not $report.baseline_available) {
        throw "performance gate failed: baseline unavailable ($baselinePath)"
    }
    if (-not $report.compare) {
        throw "performance gate failed: compare rows missing ($reportJson)"
    }

    foreach ($row in $report.compare) {
        $key = "{0}|{1}" -f $row.variant, $row.preset
        $base = [double]$row.baseline_tps
        $current = [double]$row.current_tps
        if (-not $baselineByCase.ContainsKey($key)) {
            $baselineByCase[$key] = $base
        } else {
            $prev = [double]$baselineByCase[$key]
            if ([Math]::Abs($prev - $base) -gt 0.01) {
                throw "baseline drift detected for ${key}: $prev vs $base"
            }
        }
        if (-not $samplesByCase.ContainsKey($key)) {
            $samplesByCase[$key] = @()
        }
        $samplesByCase[$key] += $current
    }

    $runReports += [ordered]@{
        run = $run
        report_json = $reportJson
        compare_pass = [bool]$report.compare_pass
    }
}

$rows = @()
$failedRows = @()
foreach ($key in @($baselineByCase.Keys | Sort-Object)) {
    $samples = @($samplesByCase[$key])
    if ($samples.Count -ne $Runs) {
        throw "sample count mismatch for ${key}: expected=$Runs actual=$($samples.Count)"
    }
    $parts = $key.Split("|", 2)
    $variantKey = $parts[0]
    $presetKey = $parts[1]
    $base = [double]$baselineByCase[$key]
    $p50 = [Math]::Round((Get-Median -Values ([double[]]$samples)), 2)
    $deltaPct = if ($base -le 0.0) { 0.0 } else { (($p50 - $base) / $base) * 100.0 }
    $pass = $deltaPct -ge $AllowedRegressionPct
    $row = [ordered]@{
        variant = $variantKey
        preset = $presetKey
        baseline_tps = [Math]::Round($base, 2)
        current_tps_p50 = $p50
        delta_pct = [Math]::Round($deltaPct, 2)
        pass = $pass
        samples = @($samples | ForEach-Object { [Math]::Round([double]$_, 2) })
    }
    $rows += [pscustomobject]$row
    if (-not $pass) {
        $failedRows += [pscustomobject]$row
    }
}

$overallPass = ($failedRows.Count -eq 0)
$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    gate_profile = "release+seal_single"
    baseline_json = $baselinePath
    allowed_regression_pct = $AllowedRegressionPct
    runs = $Runs
    variant = $Variant
    run_reports = $runReports
    compare = $rows
    pass = $overallPass
}

$summaryJson = Join-Path $OutputDir "performance-gate-summary.json"
$summaryMd = Join-Path $OutputDir "performance-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Performance Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- gate_profile: $($summary.gate_profile)"
    "- baseline_json: $($summary.baseline_json)"
    "- runs: $($summary.runs)"
    "- allowed_regression_pct: $($summary.allowed_regression_pct)"
    "- pass: $($summary.pass)"
    ""
    "## Compare (P50 Across Runs)"
    ""
    "| variant | preset | baseline_tps | current_tps_p50 | delta_pct | pass | samples |"
    "|---|---|---:|---:|---:|---|---|"
)
foreach ($row in $rows) {
    $samplesText = ($row.samples -join ",")
    $md += "| $($row.variant) | $($row.preset) | $($row.baseline_tps) | $($row.current_tps_p50) | $($row.delta_pct) | $($row.pass) | $samplesText |"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "performance gate summary:"
Write-Host "  profile: release + seal_single"
Write-Host "  baseline: $baselinePath"
Write-Host "  summary:  $summaryJson"
foreach ($row in $rows) {
    Write-Host ("  {0}/{1}: baseline={2} p50={3} delta={4}% pass={5} samples=[{6}]" -f `
        $row.variant, $row.preset, $row.baseline_tps, $row.current_tps_p50, $row.delta_pct, $row.pass, ($row.samples -join ","))
}

if (-not $overallPass) {
    $reasons = @($failedRows | ForEach-Object { "{0}/{1}:delta={2}%" -f $_.variant, $_.preset, $_.delta_pct })
    $reasonText = if ($reasons.Count -gt 0) { ($reasons -join "; ") } else { "unknown" }
    throw "performance gate FAILED: $reasonText"
}

Write-Host "performance gate PASS"
