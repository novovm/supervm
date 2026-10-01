param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(30, 1200)]
    [int]$TimeoutSeconds = 180
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\dividend-balance-source-gate"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-CargoTest {
    param(
        [string]$WorkDir,
        [string]$Filter,
        [int]$TimeoutSeconds
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $args = @("test", "-p", "novovm-consensus", $Filter, "--", "--nocapture")
    $psi.Arguments = (($args | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $proc = [System.Diagnostics.Process]::Start($psi)
    if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
        try { $proc.Kill() } catch {}
        return [ordered]@{
            exit_code = -1
            timed_out = $true
            stdout = ""
            stderr = "timed out after ${TimeoutSeconds}s"
        }
    }

    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        timed_out = $false
        stdout = $proc.StandardOutput.ReadToEnd()
        stderr = $proc.StandardError.ReadToEnd()
    }
}

$cases = @(
    [ordered]@{
        key = "market_engine_runtime_dividend_seed"
        filter = "test_market_engine_uses_runtime_dividend_balance_seed"
        category = "runtime_seed"
    },
    [ordered]@{
        key = "protocol_market_policy_syncs_dividend_balances"
        filter = "test_market_policy_reconfigure_syncs_dividend_runtime_balances"
        category = "protocol_sync"
    },
    [ordered]@{
        key = "unified_account_index_large_scale_perf"
        filter = "test_unified_account_index_refresh_large_scale_perf_budget"
        category = "perf_budget"
    }
)

$results = @()
foreach ($case in $cases) {
    $res = Invoke-CargoTest -WorkDir $RepoRoot -Filter $case.filter -TimeoutSeconds $TimeoutSeconds
    $stdoutPath = Join-Path $OutputDir "$($case.key).stdout.log"
    $stderrPath = Join-Path $OutputDir "$($case.key).stderr.log"
    $res.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
    $res.stderr | Set-Content -Path $stderrPath -Encoding UTF8
    $results += [ordered]@{
        key = $case.key
        filter = $case.filter
        category = $case.category
        pass = [bool](-not $res.timed_out -and $res.exit_code -eq 0)
        exit_code = [int]$res.exit_code
        timed_out = [bool]$res.timed_out
        stdout_log = $stdoutPath
        stderr_log = $stderrPath
    }
}

$runtimeSeedPass = @($results | Where-Object { $_.category -eq "runtime_seed" -and $_.pass }).Count -gt 0
$protocolSyncPass = @($results | Where-Object { $_.category -eq "protocol_sync" -and $_.pass }).Count -gt 0
$perfBudgetPass = @($results | Where-Object { $_.category -eq "perf_budget" -and $_.pass }).Count -gt 0
$allPass = [bool]($runtimeSeedPass -and $protocolSyncPass -and $perfBudgetPass)

$errorReason = ""
if (-not $allPass) {
    $failed = @(
        $results |
            Where-Object { -not [bool]$_["pass"] } |
            ForEach-Object { [string]$_["key"] }
    )
    $errorReason = "failed_tests: $($failed -join ',')"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $allPass
    runtime_seed_pass = $runtimeSeedPass
    protocol_sync_pass = $protocolSyncPass
    perf_budget_pass = $perfBudgetPass
    error_reason = $errorReason
    tests = $results
}

$summaryJson = Join-Path $OutputDir "dividend-balance-source-gate-summary.json"
$summaryMd = Join-Path $OutputDir "dividend-balance-source-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Dividend Balance Source Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- runtime_seed_pass: $($summary.runtime_seed_pass)"
    "- protocol_sync_pass: $($summary.protocol_sync_pass)"
    "- perf_budget_pass: $($summary.perf_budget_pass)"
    "- error_reason: $($summary.error_reason)"
    "- summary_json: $summaryJson"
    ""
    "## Tests"
)
foreach ($r in $results) {
    $md += "- $($r.key): pass=$($r.pass) exit_code=$($r.exit_code) timed_out=$($r.timed_out)"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "dividend balance source gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  runtime_seed_pass: $($summary.runtime_seed_pass)"
Write-Host "  protocol_sync_pass: $($summary.protocol_sync_pass)"
Write-Host "  perf_budget_pass: $($summary.perf_budget_pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $allPass) {
    throw "dividend balance source gate FAILED: $errorReason"
}

Write-Host "dividend balance source gate PASS"
