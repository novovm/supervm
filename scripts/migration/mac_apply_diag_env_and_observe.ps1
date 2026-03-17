param(
    [string]$RepoRoot = "",
    [string]$EnvExportJson = "",
    [UInt64]$ChainId = 1,
    [UInt64]$DurationMinutes = 3,
    [UInt64]$IntervalSeconds = 5,
    [UInt64]$WarmupSeconds = 6,
    [switch]$SkipBuild,
    [switch]$EnablePluginMempoolIngest = $true,
    [UInt64]$PluginMinCandidates = 600,
    [UInt64]$RlpxMaxPeersPerTick = 32,
    [string]$RlpxHelloProfile = "geth",
    [switch]$EnableSwapPriority = $true,
    [string]$SummaryOut = "artifacts/migration/evm-uniswap-observation-window-summary.mac-align.json"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
    }
    return (Resolve-Path $Root).Path
}

function Resolve-FullPath {
    param(
        [string]$Root,
        [string]$Value
    )
    if ([System.IO.Path]::IsPathRooted($Value)) {
        return [System.IO.Path]::GetFullPath($Value)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $Root $Value))
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
Set-Location $RepoRoot

$psExe = if (Get-Command pwsh -ErrorAction SilentlyContinue) {
    "pwsh"
} elseif (Get-Command powershell -ErrorAction SilentlyContinue) {
    "powershell"
} else {
    throw "pwsh/powershell not found"
}

if (-not $EnvExportJson) {
    throw "EnvExportJson is required (path to env-export.json from Windows diag bundle)"
}
$EnvExportJson = Resolve-FullPath -Root $RepoRoot -Value $EnvExportJson
if (-not (Test-Path $EnvExportJson)) {
    throw ("env export json not found: {0}" -f $EnvExportJson)
}

$envRows = Get-Content -Path $EnvExportJson -Raw | ConvertFrom-Json
if ($null -eq $envRows) {
    throw ("invalid env export json: {0}" -f $EnvExportJson)
}

$skipKeys = @(
    "NOVOVM_GATEWAY_BIND",
    "NOVOVM_GATEWAY_SPOOL_DIR",
    "NOVOVM_GATEWAY_MAX_REQUESTS"
)
$applied = New-Object System.Collections.ArrayList
foreach ($row in @($envRows)) {
    $name = [string]$row.Name
    $value = [string]$row.Value
    if ([string]::IsNullOrWhiteSpace($name)) { continue }
    if ($skipKeys -contains $name) { continue }
    Set-Item -Path ("Env:{0}" -f $name) -Value $value
    [void]$applied.Add($name)
}

$obsScript = Join-Path $RepoRoot "scripts/migration/run_evm_uniswap_observation_window.ps1"
if (-not (Test-Path $obsScript)) {
    throw ("missing script: {0}" -f $obsScript)
}

$args = @(
    "-ExecutionPolicy", "Bypass",
    "-File", $obsScript,
    "-ChainId", ([string][UInt64]$ChainId),
    "-DurationMinutes", ([string][UInt64]$DurationMinutes),
    "-IntervalSeconds", ([string][UInt64]$IntervalSeconds),
    "-WarmupSeconds", ([string][UInt64]$WarmupSeconds),
    "-PluginMinCandidates", ([string][UInt64]$PluginMinCandidates),
    "-RlpxMaxPeersPerTick", ([string][UInt64]$RlpxMaxPeersPerTick),
    "-RlpxHelloProfile", $RlpxHelloProfile,
    "-SummaryOut", $SummaryOut
)
if ($SkipBuild) {
    $args += "-SkipBuild"
}
if ($EnablePluginMempoolIngest) {
    $args += "-EnablePluginMempoolIngest"
}
if ($EnableSwapPriority) {
    $args += "-EnableSwapPriority"
}

Write-Host ("applied_env_count={0}" -f $applied.Count)
Write-Host ("running={0}" -f ($args -join " "))

& $psExe @args
