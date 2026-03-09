param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [string]$AllowPluginSourceTests = "true"
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

function Parse-BoolLike {
    param(
        [string]$Value,
        [bool]$Default = $false
    )

    if (-not $Value) {
        return $Default
    }
    switch ($Value.Trim().ToLowerInvariant()) {
        "1" { return $true }
        "true" { return $true }
        "yes" { return $true }
        "on" { return $true }
        "0" { return $false }
        "false" { return $false }
        "no" { return $false }
        "off" { return $false }
        default { return $Default }
    }
}

$allowPluginSourceTestsResolved = Parse-BoolLike -Value $AllowPluginSourceTests -Default $true
if ($env:NOVOVM_EVM_TX_TYPE_SIGNAL_ALLOW_PLUGIN_SOURCE_TESTS) {
    $allowPluginSourceTestsResolved = Parse-BoolLike -Value $env:NOVOVM_EVM_TX_TYPE_SIGNAL_ALLOW_PLUGIN_SOURCE_TESTS -Default $allowPluginSourceTestsResolved
}

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
        skipped = $false
        skip_reason = $null
    }
}

$cases = @()
$cases += Invoke-TestCase -Name "evm_core_translate_fields" -Args @(
    "test",
    "--manifest-path", "crates/novovm-adapter-evm-core/Cargo.toml",
    "translate_"
)
$pluginManifest = Join-Path $RepoRoot "crates\novovm-adapter-evm-plugin\Cargo.toml"
if ($allowPluginSourceTestsResolved -and (Test-Path $pluginManifest)) {
    $cases += Invoke-TestCase -Name "evm_plugin_self_guard_v2" -Args @(
        "test",
        "--manifest-path", "crates/novovm-adapter-evm-plugin/Cargo.toml",
        "plugin_apply_v2_self_guard_rejects_replay_nonce"
    )
} else {
    $skipReason = if (-not $allowPluginSourceTestsResolved) {
        "disabled by AllowPluginSourceTests/NOVOVM_EVM_TX_TYPE_SIGNAL_ALLOW_PLUGIN_SOURCE_TESTS"
    } else {
        "plugin source manifest missing: crates/novovm-adapter-evm-plugin/Cargo.toml"
    }
    $cases += [pscustomobject][ordered]@{
        name = "evm_plugin_self_guard_v2"
        pass = $true
        elapsed_ms = 0
        error = $null
        skipped = $true
        skip_reason = $skipReason
    }
}
$cases += Invoke-TestCase -Name "node_eth_raw_route_cases" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "unified_account_public_rpc_eth_send_raw_"
)
$cases += Invoke-TestCase -Name "node_eth_send_transaction_ir_cases" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "unified_account_public_rpc_eth_send_transaction_alias_"
)
$cases += Invoke-TestCase -Name "node_eth_persona_query_cases" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "unified_account_public_rpc_eth_get_transaction_count"
)
$cases += Invoke-TestCase -Name "node_eth_error_code_mapping" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "public_rpc_error_code_maps_eth_blob_and_mismatch_cases"
)
$cases += Invoke-TestCase -Name "node_eth_query_alias_receipt_tx" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "chain_query_methods_return_expected_records"
)
$cases += Invoke-TestCase -Name "node_eth_filter_reorg_m0_reject" -Args @(
    "test",
    "--manifest-path", "crates/novovm-node/Cargo.toml",
    "unified_account_public_rpc_eth_filter_reorg_methods_rejected_in_m0"
)

$overallPass = $true
foreach ($case in $cases) {
    if (-not [bool]$case.pass) {
        $overallPass = $false
        break
    }
}

$signal = [ordered]@{
    signal = "evm_tx_type_signal"
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    pass = $overallPass
    plugin_source_tests_enabled = $allowPluginSourceTestsResolved
    plugin_source_manifest_present = (Test-Path $pluginManifest)
    tests = $cases
}

$jsonPath = Join-Path $OutputDir "tx_type_compat_signal.json"
$signal | ConvertTo-Json -Depth 8 | Set-Content -Path $jsonPath -Encoding UTF8

Write-Host "evm_tx_type_signal_out: pass=$overallPass path=$jsonPath"
if (-not $overallPass) {
    throw "evm tx type signal failed"
}
