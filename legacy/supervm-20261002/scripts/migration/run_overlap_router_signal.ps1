param(
    [string]$RepoRoot = "",
    [string]$OutputDir = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\evm"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-TestCase {
    param(
        [string]$Name,
        [string[]]$Args
    )

    $start = Get-Date
    $ok = $false
    $errorText = ""
    Push-Location $RepoRoot
    try {
        & cargo @Args | Out-Null
        if ($LASTEXITCODE -eq 0) {
            $ok = $true
        } else {
            $errorText = "cargo exit code: $LASTEXITCODE"
        }
    } catch {
        $errorText = $_.Exception.Message
    } finally {
        Pop-Location
    }
    $elapsedMs = [int64]((Get-Date) - $start).TotalMilliseconds
    return [pscustomobject][ordered]@{
        name = $Name
        pass = $ok
        elapsed_ms = $elapsedMs
        error = if ($ok) { $null } else { $errorText }
    }
}

$cases = @()
$cases += Invoke-TestCase -Name "evm_overlap_class_p0" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "evm_overlap_classifies_transfer_batch_as_p0"
)
$cases += Invoke-TestCase -Name "evm_overlap_polygon_class_p0" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "evm_overlap_classifies_polygon_transfer_batch_as_p0"
)
$cases += Invoke-TestCase -Name "evm_overlap_policy_p1_plugin_first_before_compare" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "evm_overlap_policy_prefers_plugin_for_p1_before_compare_green"
)
$cases += Invoke-TestCase -Name "evm_overlap_policy_p1_native_first_after_compare" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "evm_overlap_policy_prefers_native_for_p1_after_compare_green"
)

$overallPass = $true
foreach ($case in $cases) {
    if (-not [bool]$case.pass) {
        $overallPass = $false
        break
    }
}

$signal = [ordered]@{
    signal = "overlap_router_signal"
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    pass = $overallPass
    tests = $cases
}

$jsonPath = Join-Path $OutputDir "overlap_router_signal.json"
$signal | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

Write-Host "overlap_router_signal_out: pass=$overallPass path=$jsonPath"
if (-not $overallPass) {
    throw "overlap router signal failed"
}
