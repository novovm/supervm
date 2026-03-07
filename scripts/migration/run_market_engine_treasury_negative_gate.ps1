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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\market-engine-treasury-negative-gate"
}

function Invoke-Cargo {
    param(
        [string]$WorkDir,
        [string[]]$CargoArgs
    )

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = "cargo"
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.Arguments = (($CargoArgs | ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join " ")

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdout = $proc.StandardOutput.ReadToEnd()
    $stderr = $proc.StandardError.ReadToEnd()
    $proc.WaitForExit()

    return [ordered]@{
        exit_code = [int]$proc.ExitCode
        stdout = $stdout
        stderr = $stderr
        output = ($stdout + $stderr)
    }
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$tests = @(
    [ordered]@{
        key = "web30_buyback_zero_budget_reject"
        crate = "web30-core"
        workdir = Join-Path $RepoRoot "vendor\web30-core"
        args = @("test", "--quiet", "test_buyback_zero_budget_reject", "--", "--exact")
    },
    [ordered]@{
        key = "web30_buyback_not_triggered_below_threshold"
        crate = "web30-core"
        workdir = Join-Path $RepoRoot "vendor\web30-core"
        args = @("test", "--quiet", "test_buyback_not_triggered_when_discount_below_threshold", "--", "--exact")
    },
    [ordered]@{
        key = "web30_buyback_reserve_and_burn_share"
        crate = "web30-core"
        workdir = Join-Path $RepoRoot "vendor\web30-core"
        args = @("test", "--quiet", "test_buyback_respects_min_main_reserve_and_burn_share", "--", "--exact")
    },
    [ordered]@{
        key = "web30_nav_foreign_valuation"
        crate = "web30-core"
        workdir = Join-Path $RepoRoot "vendor\web30-core"
        args = @("test", "--quiet", "test_nav_includes_foreign_reserve_valuation", "--", "--exact")
    },
    [ordered]@{
        key = "consensus_reject_zero_buyback_budget"
        crate = "novovm-consensus"
        workdir = Join-Path $RepoRoot "crates\novovm-consensus"
        args = @("test", "--quiet", "test_market_engine_rejects_zero_buyback_budget", "--", "--exact")
    }
)

$results = @()
foreach ($t in $tests) {
    $workdir = (Resolve-Path $t.workdir).Path
    $res = Invoke-Cargo -WorkDir $workdir -CargoArgs $t.args
    $stdoutPath = Join-Path $OutputDir "$($t.key).stdout.log"
    $stderrPath = Join-Path $OutputDir "$($t.key).stderr.log"
    $res.stdout | Set-Content -Path $stdoutPath -Encoding UTF8
    $res.stderr | Set-Content -Path $stderrPath -Encoding UTF8

    $results += [ordered]@{
        key = $t.key
        crate = $t.crate
        workdir = $workdir
        command = "cargo $($t.args -join ' ')"
        pass = [bool]($res.exit_code -eq 0)
        exit_code = [int]$res.exit_code
        stdout_log = $stdoutPath
        stderr_log = $stderrPath
    }
}

$allPass = @($results | Where-Object { -not $_.pass }).Count -eq 0
$errorReason = ""
if (-not $allPass) {
    $failed = @($results | Where-Object { -not $_.pass } | Select-Object -ExpandProperty key)
    $errorReason = "failed_tests: $($failed -join ',')"
}

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $allPass
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
    "- error_reason: $($summary.error_reason)"
    "- summary_json: $summaryJson"
    ""
    "## Tests"
)
foreach ($r in $results) {
    $md += "- $($r.key): pass=$($r.pass) exit_code=$($r.exit_code) crate=$($r.crate)"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "market engine treasury negative gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  error_reason: $($summary.error_reason)"
Write-Host "  summary_json: $summaryJson"

if (-not $allPass) {
    throw "market engine treasury negative gate FAILED: $errorReason"
}

Write-Host "market engine treasury negative gate PASS"
