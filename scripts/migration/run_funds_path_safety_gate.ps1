param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(30, 3600)]
    [int]$TimeoutSec = 300
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\funds-path-safety-gate"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-TestCase {
    param(
        [string]$CaseName,
        [string]$Package,
        [string]$Filter,
        [string]$Category,
        [string]$LogPath,
        [int]$TimeoutSeconds,
        [string]$WorkingDirectory
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = "test -p $Package $Filter -- --nocapture"

    $proc = [System.Diagnostics.Process]::Start($psi)
    $timedOut = -not $proc.WaitForExit($TimeoutSeconds * 1000)
    if ($timedOut) {
        try {
            $proc.Kill($true)
        } catch {
        }
    }

    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $text = ($stdout + $stderr).Trim()
    $text | Set-Content -Path $LogPath -Encoding UTF8

    $exitCode = if ($timedOut) { 124 } else { [int]$proc.ExitCode }
    $pass = (-not $timedOut) -and ($exitCode -eq 0)
    return [ordered]@{
        case_name = $CaseName
        package = $Package
        filter = $Filter
        category = $Category
        timeout_seconds = $TimeoutSeconds
        timed_out = $timedOut
        exit_code = $exitCode
        pass = $pass
        log = $LogPath
    }
}

$cases = @(
    [ordered]@{
        case_name = "settlement_query_in_memory_index"
        package = "novovm-evm-gateway"
        filter = "evm_settlement_query_methods_hit_in_memory_index"
        category = "reconcile"
    },
    [ordered]@{
        case_name = "replay_settlement_payout"
        package = "novovm-evm-gateway"
        filter = "evm_replay_settlement_payout_clears_pending_and_updates_status"
        category = "compensation"
    },
    [ordered]@{
        case_name = "auto_replay_pending_payouts_cap"
        package = "novovm-evm-gateway"
        filter = "auto_replay_pending_payouts_respects_cap_and_advances_status"
        category = "compensation"
    },
    [ordered]@{
        case_name = "replay_atomic_ready"
        package = "novovm-evm-gateway"
        filter = "evm_replay_atomic_ready_clears_pending_and_updates_status"
        category = "compensation"
    },
    [ordered]@{
        case_name = "atomic_broadcast_failed_then_replay"
        package = "novovm-evm-gateway"
        filter = "evm_mark_failed_and_replay_atomic_broadcast_queue_updates_status"
        category = "failure_injection"
    },
    [ordered]@{
        case_name = "public_broadcast_failure_error_shape"
        package = "novovm-evm-gateway"
        filter = "gateway_error_code_and_data_for_public_broadcast_failure"
        category = "failure_injection"
    },
    [ordered]@{
        case_name = "submit_status_failure_when_tx_missing"
        package = "novovm-evm-gateway"
        filter = "evm_get_tx_submit_status_uses_persisted_failure_status_when_tx_missing"
        category = "invariant"
    },
    [ordered]@{
        case_name = "submit_status_success_when_tx_missing"
        package = "novovm-evm-gateway"
        filter = "evm_get_tx_submit_status_uses_persisted_success_status_when_tx_missing"
        category = "invariant"
    },
    [ordered]@{
        case_name = "submit_status_onchain_failed_when_tx_missing"
        package = "novovm-evm-gateway"
        filter = "evm_get_tx_submit_status_uses_persisted_onchain_failed_status_when_tx_missing"
        category = "invariant"
    },
    [ordered]@{
        case_name = "settlement_opswire_record_count"
        package = "novovm-evm-gateway"
        filter = "encode_gateway_evm_settlement_ops_wire_tracks_record_count"
        category = "invariant"
    },
    [ordered]@{
        case_name = "payout_opswire_instruction_count"
        package = "novovm-evm-gateway"
        filter = "encode_gateway_evm_payout_ops_wire_tracks_instruction_count"
        category = "invariant"
    },
    [ordered]@{
        case_name = "atomic_ready_opswire_record_count"
        package = "novovm-evm-gateway"
        filter = "encode_gateway_evm_atomic_ready_ops_wire_tracks_record_count"
        category = "invariant"
    }
)

$results = @()
foreach ($case in $cases) {
    $logPath = Join-Path $OutputDir "$($case.case_name).log"
    $result = Invoke-TestCase `
        -CaseName $case.case_name `
        -Package $case.package `
        -Filter $case.filter `
        -Category $case.category `
        -LogPath $logPath `
        -TimeoutSeconds $TimeoutSec `
        -WorkingDirectory $RepoRoot
    $results += $result
}

function Category-Pass {
    param(
        [array]$Rows,
        [string]$Category
    )
    $subset = @($Rows | Where-Object { $_.category -eq $Category })
    if ($subset.Count -eq 0) {
        return $false
    }
    return [bool](@($subset | Where-Object { -not $_.pass }).Count -eq 0)
}

$reconcilePass = Category-Pass -Rows $results -Category "reconcile"
$compensationPass = Category-Pass -Rows $results -Category "compensation"
$failureInjectionPass = Category-Pass -Rows $results -Category "failure_injection"
$invariantPass = Category-Pass -Rows $results -Category "invariant"
$allPass = [bool]($reconcilePass -and $compensationPass -and $failureInjectionPass -and $invariantPass)

$errorReason = ""
if (-not $allPass) {
    $failed = @(
        $results |
            Where-Object { -not [bool]$_["pass"] } |
            ForEach-Object { [string]$_["case_name"] }
    )
    $errorReason = "failed_cases: $($failed -join ',')"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $allPass
    timeout_seconds = $TimeoutSec
    reconcile_pass = $reconcilePass
    compensation_pass = $compensationPass
    failure_injection_pass = $failureInjectionPass
    invariant_pass = $invariantPass
    error_reason = $errorReason
    cases = $results
}

$summaryJson = Join-Path $OutputDir "funds-path-safety-gate-summary.json"
$summaryMd = Join-Path $OutputDir "funds-path-safety-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Funds Path Safety Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- timeout_seconds: $($summary.timeout_seconds)"
    "- reconcile_pass: $($summary.reconcile_pass)"
    "- compensation_pass: $($summary.compensation_pass)"
    "- failure_injection_pass: $($summary.failure_injection_pass)"
    "- invariant_pass: $($summary.invariant_pass)"
    "- error_reason: $($summary.error_reason)"
    ""
    "## Cases"
    ""
    "| case | category | pass | timed_out | exit_code | log |"
    "|---|---|---|---|---:|---|"
)
foreach ($case in $summary.cases) {
    $md += "| $($case.case_name) | $($case.category) | $($case.pass) | $($case.timed_out) | $($case.exit_code) | $($case.log) |"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "funds path safety gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  reconcile_pass: $($summary.reconcile_pass)"
Write-Host "  compensation_pass: $($summary.compensation_pass)"
Write-Host "  failure_injection_pass: $($summary.failure_injection_pass)"
Write-Host "  invariant_pass: $($summary.invariant_pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md:   $summaryMd"

if (-not $summary.pass) {
    throw "funds path safety gate FAILED"
}

Write-Host "funds path safety gate PASS"
