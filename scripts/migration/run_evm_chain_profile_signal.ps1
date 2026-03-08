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
$cases += Invoke-TestCase -Name "evm_core_profile_family_support" -Args @(
    "test",
    "--manifest-path", "crates/novovm-adapter-evm-core/Cargo.toml",
    "supports_evm_family_includes_polygon_and_avalanche"
)
$cases += Invoke-TestCase -Name "evm_core_profile_resolver_m0_family" -Args @(
    "test",
    "--manifest-path", "crates/novovm-adapter-evm-core/Cargo.toml",
    "resolve_profile_supports_m0_evm_family"
)
$cases += Invoke-TestCase -Name "native_adapter_profile_family_support" -Args @(
    "test",
    "--manifest-path", "crates/novovm-adapter-novovm/Cargo.toml",
    "supports_native_chain_includes_polygon_and_avalanche"
)
$cases += Invoke-TestCase -Name "node_default_chain_id_profile_mapping" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "adapter_chain_id_mapping_is_stable"
)

$overallPass = $true
foreach ($case in $cases) {
    if (-not [bool]$case.pass) {
        $overallPass = $false
        break
    }
}

$signal = [ordered]@{
    signal = "evm_chain_profile_signal"
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    pass = $overallPass
    tests = $cases
}

$jsonPath = Join-Path $OutputDir "evm_chain_profile_signal.json"
$signal | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

Write-Host "evm_chain_profile_signal_out: pass=$overallPass path=$jsonPath"
if (-not $overallPass) {
    throw "evm chain profile signal failed"
}
