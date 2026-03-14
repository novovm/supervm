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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\market-engine-treasury-negative-gate"
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
        key = "market_policy_clamped_values_rejected"
        filter = "test_market_engine_rejects_clamped_policy_values"
        category = "policy_negative"
    },
    [ordered]@{
        key = "market_policy_zero_buyback_budget_rejected"
        filter = "test_market_engine_rejects_zero_buyback_budget"
        category = "policy_negative"
    },
    [ordered]@{
        key = "governance_treasury_spend_flow"
        filter = "test_governance_execute_treasury_spend"
        category = "treasury_path"
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

$policyNegativePass = @($results | Where-Object { $_.category -eq "policy_negative" -and $_.pass }).Count -eq 2
$treasuryPathPass = @($results | Where-Object { $_.category -eq "treasury_path" -and $_.pass }).Count -eq 1
$allPass = [bool]($policyNegativePass -and $treasuryPathPass)

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
    policy_negative_pass = $policyNegativePass
    treasury_path_pass = $treasuryPathPass
    error_reason = $errorReason
    tests = $results
}

$summaryJson = Join-Path $OutputDir "market-engine-treasury-negative-gate-summary.json"
$summaryMd = Join-Path $OutputDir "market-engine-treasury-negative-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Market Engine Treasury Negative Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- policy_negative_pass: $($summary.policy_negative_pass)"
    "- treasury_path_pass: $($summary.treasury_path_pass)"
    "- error_reason: $($summary.error_reason)"
    "- summary_json: $summaryJson"
    ""
    "## Tests"
)
foreach ($r in $results) {
    $md += "- $($r.key): pass=$($r.pass) exit_code=$($r.exit_code) timed_out=$($r.timed_out)"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "market engine treasury negative gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  policy_negative_pass: $($summary.policy_negative_pass)"
Write-Host "  treasury_path_pass: $($summary.treasury_path_pass)"
Write-Host "  summary_json: $summaryJson"

if (-not $allPass) {
    throw "market engine treasury negative gate FAILED: $errorReason"
}

Write-Host "market engine treasury negative gate PASS"
