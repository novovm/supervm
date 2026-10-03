param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(30, 3600)]
    [int]$TimeoutSeconds = 420
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\ops-control-surface-gate"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Require-Path {
    param(
        [string]$Path,
        [string]$Name
    )
    if (-not (Test-Path $Path)) {
        throw "missing ${Name}: $Path"
    }
}

function Read-JsonFile {
    param([string]$Path)

    Require-Path -Path $Path -Name "json"
    $raw = Get-Content -Path $Path -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        throw "json is empty: $Path"
    }
    $convertFromJsonCmd = Get-Command ConvertFrom-Json -ErrorAction Stop
    $hasDepth = $convertFromJsonCmd.Parameters.ContainsKey("Depth")
    if ($hasDepth) {
        return ($raw | ConvertFrom-Json -Depth 64)
    }
    return ($raw | ConvertFrom-Json)
}

function Invoke-TestCase {
    param(
        [string]$CaseName,
        [string]$Package,
        [string]$Filter,
        [string]$Category,
        [string]$LogPath,
        [int]$TimeoutSec,
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
    $timedOut = -not $proc.WaitForExit($TimeoutSec * 1000)
    if ($timedOut) {
        try {
            $proc.Kill($true)
        } catch {
        }
    }

    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    ($stdout + $stderr).Trim() | Set-Content -Path $LogPath -Encoding UTF8

    $exitCode = if ($timedOut) { 124 } else { [int]$proc.ExitCode }
    $pass = (-not $timedOut) -and ($exitCode -eq 0)
    return [ordered]@{
        case_name = $CaseName
        package = $Package
        filter = $Filter
        category = $Category
        timeout_seconds = $TimeoutSec
        timed_out = $timedOut
        exit_code = $exitCode
        pass = $pass
        log = $LogPath
    }
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

$runtimeGateScript = Join-Path $RepoRoot "scripts\migration\run_runtime_security_baseline_gate.ps1"
Require-Path -Path $runtimeGateScript -Name "runtime security baseline gate script"

$runtimeOutputDir = Join-Path $OutputDir "runtime-security-baseline-gate"
New-Item -ItemType Directory -Force -Path $runtimeOutputDir | Out-Null

Write-Host "ops control surface gate: runtime security baseline ..."
& $runtimeGateScript `
    -RepoRoot $RepoRoot `
    -OutputDir $runtimeOutputDir `
    -TimeoutSec $TimeoutSeconds

$runtimeJsonPath = Join-Path $runtimeOutputDir "runtime-security-baseline-gate-summary.json"
$runtimeSummary = Read-JsonFile -Path $runtimeJsonPath

$runtimePass = [bool]$runtimeSummary.pass
$rateLimitPass = [bool]$runtimeSummary.rate_limit_pass
$auditFieldPass = [bool]$runtimeSummary.audit_log_integrity_pass

$cases = @(
    [ordered]@{
        case_name = "governance_access_multisig_timelock_circuit_breaker"
        package = "novovm-consensus"
        filter = "test_governance_access_policy_multisig_and_timelock"
        category = "circuit_breaker"
    },
    [ordered]@{
        case_name = "governance_mempool_fee_floor_quota"
        package = "novovm-consensus"
        filter = "test_governance_execute_update_mempool_fee_floor"
        category = "quota"
    },
    [ordered]@{
        case_name = "governance_market_policy_quota_controls"
        package = "novovm-consensus"
        filter = "test_governance_execute_update_market_governance_policy"
        category = "quota"
    },
    [ordered]@{
        case_name = "foreign_rate_source_fallback_alert_signal"
        package = "novovm-consensus"
        filter = "test_foreign_rate_source_external_missing_quote_fallback"
        category = "alert_fields"
    },
    [ordered]@{
        case_name = "foreign_rate_source_invalid_quote_reject_signal"
        package = "novovm-consensus"
        filter = "test_foreign_rate_source_reject_invalid_quote_spec"
        category = "alert_fields"
    }
)

$results = @()
foreach ($case in $cases) {
    $logPath = Join-Path $OutputDir "$($case.case_name).log"
    $results += Invoke-TestCase `
        -CaseName $case.case_name `
        -Package $case.package `
        -Filter $case.filter `
        -Category $case.category `
        -LogPath $logPath `
        -TimeoutSec $TimeoutSeconds `
        -WorkingDirectory $RepoRoot
}

$circuitBreakerPass = Category-Pass -Rows $results -Category "circuit_breaker"
$quotaPass = Category-Pass -Rows $results -Category "quota"
$alertFieldPass = Category-Pass -Rows $results -Category "alert_fields"

$pass = [bool](
    $runtimePass -and
    $rateLimitPass -and
    $auditFieldPass -and
    $circuitBreakerPass -and
    $quotaPass -and
    $alertFieldPass
)

$errorReason = ""
if (-not $runtimePass) {
    $errorReason = "runtime_security_gate_failed"
} elseif (-not $rateLimitPass) {
    $errorReason = "rate_limit_assertion_failed"
} elseif (-not $circuitBreakerPass) {
    $errorReason = "circuit_breaker_assertion_failed"
} elseif (-not $quotaPass) {
    $errorReason = "quota_assertion_failed"
} elseif (-not $alertFieldPass) {
    $errorReason = "alert_field_assertion_failed"
} elseif (-not $auditFieldPass) {
    $errorReason = "audit_field_assertion_failed"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $pass
    timeout_seconds = $TimeoutSeconds
    runtime_security_gate_pass = $runtimePass
    rate_limit_pass = $rateLimitPass
    circuit_breaker_pass = $circuitBreakerPass
    quota_pass = $quotaPass
    alert_field_pass = $alertFieldPass
    audit_field_pass = $auditFieldPass
    error_reason = $errorReason
    evidence = [ordered]@{
        runtime_security_summary_json = $runtimeJsonPath
        cases = $results
    }
}

$summaryJson = Join-Path $OutputDir "ops-control-surface-gate-summary.json"
$summaryMd = Join-Path $OutputDir "ops-control-surface-gate-summary.md"
$summary | ConvertTo-Json -Depth 10 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Ops Control Surface Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- timeout_seconds: $($summary.timeout_seconds)"
    "- runtime_security_gate_pass: $($summary.runtime_security_gate_pass)"
    "- rate_limit_pass: $($summary.rate_limit_pass)"
    "- circuit_breaker_pass: $($summary.circuit_breaker_pass)"
    "- quota_pass: $($summary.quota_pass)"
    "- alert_field_pass: $($summary.alert_field_pass)"
    "- audit_field_pass: $($summary.audit_field_pass)"
    "- error_reason: $($summary.error_reason)"
    ""
    "## Evidence"
    ""
    "- runtime_security_summary_json: $($summary.evidence.runtime_security_summary_json)"
    "- summary_json: $summaryJson"
    ""
    "## Cases"
    ""
    "| case | category | pass | timed_out | exit_code | log |"
    "|---|---|---|---|---:|---|"
)
foreach ($case in $results) {
    $md += "| $($case.case_name) | $($case.category) | $($case.pass) | $($case.timed_out) | $($case.exit_code) | $($case.log) |"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "ops control surface gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  rate_limit_pass: $($summary.rate_limit_pass)"
Write-Host "  circuit_breaker_pass: $($summary.circuit_breaker_pass)"
Write-Host "  quota_pass: $($summary.quota_pass)"
Write-Host "  alert_field_pass: $($summary.alert_field_pass)"
Write-Host "  audit_field_pass: $($summary.audit_field_pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md:   $summaryMd"

if (-not $summary.pass) {
    throw "ops control surface gate FAILED: $errorReason"
}

Write-Host "ops control surface gate PASS"
