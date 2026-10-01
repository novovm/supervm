[CmdletBinding()]
param(
    [string]$DayLabel = "",
    [string]$OutputDir = "",
    [string]$NativeExecutionStorePath = "",
    [string]$RpcUrl = "http://127.0.0.1:8899",
    [string]$MainlineQueryStorePath = "",
    [switch]$NoReset
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

function Ensure-Directory {
    param([Parameter(Mandatory = $true)][string]$Path)
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
}

$repoRoot = Resolve-RepoRoot
if ([string]::IsNullOrWhiteSpace($DayLabel)) {
    $DayLabel = (Get-Date).ToString("yyyy-MM-dd")
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputDir = Join-Path $repoRoot "artifacts/mainline/p2d-run-phase/$DayLabel"
}
Ensure-Directory -Path $OutputDir

if ([string]::IsNullOrWhiteSpace($NativeExecutionStorePath)) {
    $NativeExecutionStorePath = Join-Path $OutputDir "native-execution-store.json"
}

$injectArgs = @(
    "run", "-p", "novovm-node", "--bin", "supervm-mainline-p2d-sample-inject", "--",
    "--store-path", $NativeExecutionStorePath
)
if ($NoReset) {
    $injectArgs += "--no-reset"
}

Push-Location $repoRoot
try {
    & cargo @injectArgs
    if ($LASTEXITCODE -ne 0) {
        throw "sample injector failed (exit=$LASTEXITCODE)"
    }
} finally {
    Pop-Location
}

$reportScript = Join-Path $repoRoot "scripts/novovm-p2d-daily-report.ps1"
$reportArgs = @{
    RpcUrl = $RpcUrl
    DayLabel = $DayLabel
    OutputDir = $OutputDir
    NativeExecutionStorePath = $NativeExecutionStorePath
}
if (-not [string]::IsNullOrWhiteSpace($MainlineQueryStorePath)) {
    $reportArgs["MainlineQueryStorePath"] = $MainlineQueryStorePath
}

& powershell -NoProfile -ExecutionPolicy Bypass -File $reportScript @reportArgs
if ($LASTEXITCODE -ne 0) {
    throw "p2d daily report generation failed (exit=$LASTEXITCODE)"
}
