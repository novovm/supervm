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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\runtime-security-baseline-gate"
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
        case_name = "evm_gateway_native_discovery_rate_limit"
        package = "novovm-evm-gateway"
        filter = "native_discovery_send_is_rate_limited"
        category = "rate_limit"
    },
    [ordered]@{
        case_name = "consensus_governance_network_dos_policy_update"
        package = "novovm-consensus"
        filter = "test_governance_execute_update_network_dos_policy"
        category = "rate_limit"
    },
    [ordered]@{
        case_name = "ua_delegate_policy_acl_denied"
        package = "novovm-node"
        filter = "unified_account_gate_ua_g08_permission_delegate_cannot_update_policy"
        category = "acl"
    },
    [ordered]@{
        case_name = "ua_audit_jsonl_append_integrity"
        package = "novovm-node"
        filter = "unified_account_audit_sink_appends_jsonl_records"
        category = "audit_log_integrity"
    },
    [ordered]@{
        case_name = "ua_audit_rocksdb_append_integrity"
        package = "novovm-node"
        filter = "unified_account_audit_sink_appends_rocksdb_records"
        category = "audit_log_integrity"
    },
    [ordered]@{
        case_name = "ua_audit_rpc_filter_integrity"
        package = "novovm-node"
        filter = "unified_account_public_rpc_get_audit_events_from_sink_supports_filters"
        category = "audit_log_integrity"
    },
    [ordered]@{
        case_name = "ua_audit_migration_incremental_integrity"
        package = "novovm-node"
        filter = "unified_account_audit_migration_jsonl_to_rocksdb_is_incremental"
        category = "audit_log_integrity"
    },
    [ordered]@{
        case_name = "consensus_chain_audit_records_integrity"
        package = "novovm-consensus"
        filter = "test_governance_chain_audit_records_submit_and_execute"
        category = "audit_log_integrity"
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

$rateLimitPass = Category-Pass -Rows $results -Category "rate_limit"
$aclPass = Category-Pass -Rows $results -Category "acl"
$auditLogIntegrityPass = Category-Pass -Rows $results -Category "audit_log_integrity"
$allPass = [bool]($rateLimitPass -and $aclPass -and $auditLogIntegrityPass)

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
    rate_limit_pass = $rateLimitPass
    acl_pass = $aclPass
    audit_log_integrity_pass = $auditLogIntegrityPass
    error_reason = $errorReason
    cases = $results
}

$summaryJson = Join-Path $OutputDir "runtime-security-baseline-gate-summary.json"
$summaryMd = Join-Path $OutputDir "runtime-security-baseline-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Runtime Security Baseline Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- timeout_seconds: $($summary.timeout_seconds)"
    "- rate_limit_pass: $($summary.rate_limit_pass)"
    "- acl_pass: $($summary.acl_pass)"
    "- audit_log_integrity_pass: $($summary.audit_log_integrity_pass)"
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

Write-Host "runtime security baseline gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  rate_limit_pass: $($summary.rate_limit_pass)"
Write-Host "  acl_pass: $($summary.acl_pass)"
Write-Host "  audit_log_integrity_pass: $($summary.audit_log_integrity_pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md:   $summaryMd"

if (-not $summary.pass) {
    throw "runtime security baseline gate FAILED"
}

Write-Host "runtime security baseline gate PASS"
