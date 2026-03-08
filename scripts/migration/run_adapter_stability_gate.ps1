param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(2, 20)]
    [int]$Runs = 3,
    [ValidateSet("core", "persist", "wasm")]
    [string]$CapabilityVariant = "persist"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\adapter-stability-gate"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)

function Get-Bool {
    param([object]$Value)
    if ($null -eq $Value) {
        return $false
    }
    return [bool]$Value
}

function Get-Int64OrZero {
    param([object]$Value)
    if ($null -eq $Value) {
        return [int64]0
    }
    return [int64]$Value
}

function Test-RegistryNegativeHashMismatchFlake {
    param([object]$Raw)

    if ($null -eq $Raw -or $null -eq $Raw.adapter_plugin_registry_negative_signal) {
        return $false
    }

    $signal = $Raw.adapter_plugin_registry_negative_signal
    if (Get-Bool $signal.pass) {
        return $false
    }

    $hashFailedAsExpected = Get-Bool $signal.hash_mismatch.failed_as_expected
    $hashReasonMatch = Get-Bool $signal.hash_mismatch.reason_match
    $whitelistFailedAsExpected = Get-Bool $signal.whitelist_mismatch.failed_as_expected
    $whitelistReasonMatch = Get-Bool $signal.whitelist_mismatch.reason_match
    $hashExitCode = if ($null -eq $signal.hash_mismatch.exit_code) { 0 } else { [int]$signal.hash_mismatch.exit_code }

    # Known flake shape:
    # hash-mismatch negative exits non-zero but reason line is missing (e.g. process crash),
    # while whitelist mismatch branch still behaves as expected.
    return (
        $hashFailedAsExpected -and
        (-not $hashReasonMatch) -and
        ($hashExitCode -ne 0) -and
        $whitelistFailedAsExpected -and
        $whitelistReasonMatch
    )
}

function Test-WasmDigestAccessViolationFlake {
    param([string]$Message)

    if ([string]::IsNullOrWhiteSpace($Message)) {
        return $false
    }

    return (
        ($Message -match "ffi_consistency_digest") -and
        ($Message -match "STATUS_ACCESS_VIOLATION")
    )
}

function Test-AbiNegativeReasonDriftFlake {
    param([object]$Raw)

    if ($null -eq $Raw -or $null -eq $Raw.adapter_plugin_abi_negative_signal) {
        return $false
    }

    $signal = $Raw.adapter_plugin_abi_negative_signal
    if (Get-Bool $signal.pass) {
        return $false
    }

    $abiFailedAsExpected = Get-Bool $signal.abi_mismatch.failed_as_expected
    $abiReasonMatch = Get-Bool $signal.abi_mismatch.reason_match
    $abiExitCode = if ($null -eq $signal.abi_mismatch.exit_code) { 0 } else { [int]$signal.abi_mismatch.exit_code }
    $capFailedAsExpected = Get-Bool $signal.capability_mismatch.failed_as_expected
    $capReasonMatch = Get-Bool $signal.capability_mismatch.reason_match

    return (
        $abiFailedAsExpected -and
        (-not $abiReasonMatch) -and
        ($abiExitCode -ne 0) -and
        $capFailedAsExpected -and
        $capReasonMatch
    )
}

$functionalScript = Join-Path $RepoRoot "scripts\migration\run_functional_consistency.ps1"
if (-not (Test-Path $functionalScript)) {
    throw "missing functional consistency script: $functionalScript"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$runReports = @()
for ($i = 1; $i -le $Runs; $i++) {
    $runDir = Join-Path $OutputDir ("run-" + $i)
    New-Item -ItemType Directory -Force -Path $runDir | Out-Null

    $maxRetry = 2
    $attempt = 0
    $retryUsed = $false
    $retryReason = ""
    $attemptDir = $runDir
    $jsonPath = ""
    $raw = $null

    while ($true) {
        if ($attempt -eq 0) {
            $attemptDir = $runDir
        } else {
            $attemptDir = Join-Path $runDir ("retry-" + $attempt)
            New-Item -ItemType Directory -Force -Path $attemptDir | Out-Null
        }

        $invokeFailed = $false
        $invokeError = ""
        try {
            & $functionalScript `
                -RepoRoot $RepoRoot `
                -OutputDir $attemptDir `
                -CapabilityVariant $CapabilityVariant | Out-Null
        } catch {
            $invokeFailed = $true
            $invokeError = $_.Exception.Message
        }
        if ($invokeFailed) {
            if (($attempt -lt $maxRetry) -and (Test-WasmDigestAccessViolationFlake -Message $invokeError)) {
                $retryUsed = $true
                $retryReason = "variant_digest_wasm_access_violation"
                $attempt++
                Write-Host "adapter stability gate: run-${i} retry due to wasm digest access violation (attempt=$attempt)"
                continue
            }
            throw $invokeError
        }
        $jsonPath = Join-Path $attemptDir "functional-consistency.json"
        if (-not (Test-Path $jsonPath)) {
            throw "missing functional consistency json for run-${i} attempt-${attempt}: $jsonPath"
        }
        $raw = Get-Content -Path $jsonPath -Raw | ConvertFrom-Json

        $adapterPass = Get-Bool $raw.adapter_signal.pass
        $abiPass = Get-Bool $raw.adapter_plugin_abi_signal.pass
        $registryPass = Get-Bool $raw.adapter_plugin_registry_signal.pass
        $consensusPass = Get-Bool $raw.adapter_consensus_binding_signal.pass
        $comparePass = Get-Bool $raw.adapter_backend_compare_signal.pass
        $compareAvailable = Get-Bool $raw.adapter_backend_compare_signal.available
        $compareStateRootEqual = Get-Bool $raw.adapter_backend_compare_signal.state_root_equal
        $abiNegativePass = Get-Bool $raw.adapter_plugin_abi_negative_signal.pass
        $symbolNegativePass = Get-Bool $raw.adapter_plugin_symbol_negative_signal.pass
        $registryNegativePass = Get-Bool $raw.adapter_plugin_registry_negative_signal.pass

        $adapterGatePass = (
            $adapterPass -and
            $abiPass -and
            $registryPass -and
            $consensusPass -and
            $compareAvailable -and
            $comparePass -and
            $compareStateRootEqual -and
            $abiNegativePass -and
            $symbolNegativePass -and
            $registryNegativePass
        )

        if ($adapterGatePass) {
            break
        }

        if ($attempt -ge $maxRetry) {
            break
        }

        if (Test-RegistryNegativeHashMismatchFlake -Raw $raw) {
            $retryUsed = $true
            $retryReason = "registry_negative_hash_mismatch_reason_drift"
            $attempt++
            Write-Host "adapter stability gate: run-${i} retry due to known registry-negative flake (attempt=$attempt)"
            continue
        }

        if (Test-AbiNegativeReasonDriftFlake -Raw $raw) {
            $retryUsed = $true
            $retryReason = "abi_negative_reason_drift"
            $attempt++
            Write-Host "adapter stability gate: run-${i} retry due to known abi-negative reason drift (attempt=$attempt)"
            continue
        }

        break
    }

    $adapterPass = Get-Bool $raw.adapter_signal.pass
    $abiPass = Get-Bool $raw.adapter_plugin_abi_signal.pass
    $registryPass = Get-Bool $raw.adapter_plugin_registry_signal.pass
    $consensusPass = Get-Bool $raw.adapter_consensus_binding_signal.pass
    $comparePass = Get-Bool $raw.adapter_backend_compare_signal.pass
    $compareAvailable = Get-Bool $raw.adapter_backend_compare_signal.available
    $compareStateRootEqual = Get-Bool $raw.adapter_backend_compare_signal.state_root_equal
    $abiNegativePass = Get-Bool $raw.adapter_plugin_abi_negative_signal.pass
    $symbolNegativePass = Get-Bool $raw.adapter_plugin_symbol_negative_signal.pass
    $registryNegativePass = Get-Bool $raw.adapter_plugin_registry_negative_signal.pass

    $nativeElapsedUs = Get-Int64OrZero $raw.adapter_backend_compare_signal.native.node.elapsed_us
    $pluginElapsedUs = Get-Int64OrZero $raw.adapter_backend_compare_signal.plugin.node.elapsed_us

    $adapterGatePass = (
        $adapterPass -and
        $abiPass -and
        $registryPass -and
        $consensusPass -and
        $compareAvailable -and
        $comparePass -and
        $compareStateRootEqual -and
        $abiNegativePass -and
        $symbolNegativePass -and
        $registryNegativePass
    )

    $runReports += [ordered]@{
        run = $i
        adapter_gate_pass = $adapterGatePass
        functional_overall_pass = Get-Bool $raw.overall_pass
        adapter_pass = $adapterPass
        plugin_abi_pass = $abiPass
        plugin_registry_pass = $registryPass
        consensus_binding_pass = $consensusPass
        compare_available = $compareAvailable
        compare_pass = $comparePass
        compare_state_root_equal = $compareStateRootEqual
        abi_negative_pass = $abiNegativePass
        symbol_negative_pass = $symbolNegativePass
        registry_negative_pass = $registryNegativePass
        native_elapsed_us = $nativeElapsedUs
        plugin_elapsed_us = $pluginElapsedUs
        retry_used = $retryUsed
        retry_count = $attempt
        retry_reason = $retryReason
        selected_attempt_dir = $attemptDir
        functional_json = $jsonPath
    }
}

$passCount = @($runReports | Where-Object { $_.adapter_gate_pass }).Count
$passRate = [Math]::Round(($passCount * 100.0) / $Runs, 2)
$nativeElapsedList = @($runReports | ForEach-Object { [int64]$_.native_elapsed_us })
$pluginElapsedList = @($runReports | ForEach-Object { [int64]$_.plugin_elapsed_us })
$nativeAvg = if ($nativeElapsedList.Count -gt 0) { [Math]::Round((($nativeElapsedList | Measure-Object -Sum).Sum / [double]$nativeElapsedList.Count), 2) } else { 0.0 }
$pluginAvg = if ($pluginElapsedList.Count -gt 0) { [Math]::Round((($pluginElapsedList | Measure-Object -Sum).Sum / [double]$pluginElapsedList.Count), 2) } else { 0.0 }

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    runs = $Runs
    pass = ($passCount -eq $Runs)
    pass_count = $passCount
    pass_rate_pct = $passRate
    adapter_gate_required = @(
        "adapter_signal.pass",
        "adapter_plugin_abi_signal.pass",
        "adapter_plugin_registry_signal.pass",
        "adapter_consensus_binding_signal.pass",
        "adapter_backend_compare_signal.available/pass/state_root_equal",
        "adapter_plugin_abi_negative_signal.pass",
        "adapter_plugin_symbol_negative_signal.pass",
        "adapter_plugin_registry_negative_signal.pass"
    )
    compare_elapsed_us = [ordered]@{
        native_min = if ($nativeElapsedList.Count -gt 0) { ($nativeElapsedList | Measure-Object -Minimum).Minimum } else { 0 }
        native_max = if ($nativeElapsedList.Count -gt 0) { ($nativeElapsedList | Measure-Object -Maximum).Maximum } else { 0 }
        native_avg = $nativeAvg
        plugin_min = if ($pluginElapsedList.Count -gt 0) { ($pluginElapsedList | Measure-Object -Minimum).Minimum } else { 0 }
        plugin_max = if ($pluginElapsedList.Count -gt 0) { ($pluginElapsedList | Measure-Object -Maximum).Maximum } else { 0 }
        plugin_avg = $pluginAvg
    }
    run_reports = $runReports
}

$summaryJson = Join-Path $OutputDir "adapter-stability-summary.json"
$summaryMd = Join-Path $OutputDir "adapter-stability-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Adapter Stability Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- runs: $($summary.runs)"
    "- pass: $($summary.pass)"
    "- pass_count: $($summary.pass_count)"
    "- pass_rate_pct: $($summary.pass_rate_pct)"
    "- native_elapsed_us(min/max/avg): $($summary.compare_elapsed_us.native_min)/$($summary.compare_elapsed_us.native_max)/$($summary.compare_elapsed_us.native_avg)"
    "- plugin_elapsed_us(min/max/avg): $($summary.compare_elapsed_us.plugin_min)/$($summary.compare_elapsed_us.plugin_max)/$($summary.compare_elapsed_us.plugin_avg)"
    ""
    "## Run Reports"
    ""
    "| run | adapter_gate_pass | compare_pass | abi_negative | symbol_negative | registry_negative | native_elapsed_us | plugin_elapsed_us |"
    "|---|---|---|---|---|---|---|---|"
)
foreach ($r in $runReports) {
    $md += "| $($r.run) | $($r.adapter_gate_pass) | $($r.compare_pass) | $($r.abi_negative_pass) | $($r.symbol_negative_pass) | $($r.registry_negative_pass) | $($r.native_elapsed_us) | $($r.plugin_elapsed_us) |"
}
$retryRuns = @($runReports | Where-Object { $_.retry_used })
if ($retryRuns.Count -gt 0) {
    $md += ""
    $md += "## Retry Events"
    $md += ""
    foreach ($r in $retryRuns) {
        $md += "- run=$($r.run) retry_count=$($r.retry_count) retry_reason=$($r.retry_reason) selected_attempt_dir=$($r.selected_attempt_dir)"
    }
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "adapter stability gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  pass_rate_pct: $($summary.pass_rate_pct)"
Write-Host "  summary_json: $summaryJson"

if (-not $summary.pass) {
    throw "adapter stability gate FAILED (pass_count=$passCount/$Runs)"
}

Write-Host "adapter stability gate PASS"
