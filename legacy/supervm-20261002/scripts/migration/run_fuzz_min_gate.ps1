param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(30, 3600)]
    [int]$TimeoutSec = 300,
    [ValidateRange(1, 18446744073709551615)]
    [UInt64]$Seed = 20260313,
    [ValidateRange(1, 1000000)]
    [int]$TxWireIterations = 5000,
    [ValidateRange(1, 1000000)]
    [int]$RpcIterations = 3000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\fuzz-min-gate"
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
        [hashtable]$EnvMap,
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

    foreach ($entry in $EnvMap.GetEnumerator()) {
        $psi.Environment[$entry.Key] = [string]$entry.Value
    }

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
        timeout_seconds = $TimeoutSeconds
        timed_out = $timedOut
        exit_code = $exitCode
        pass = $pass
        log = $LogPath
    }
}

$commonEnv = @{
    NOVOVM_FUZZ_MIN_SEED = "$Seed"
}
$txEnv = @{}
foreach ($k in $commonEnv.Keys) {
    $txEnv[$k] = $commonEnv[$k]
}
$txEnv["NOVOVM_FUZZ_MIN_TX_ITERS"] = "$TxWireIterations"

$rpcEnv = @{}
foreach ($k in $commonEnv.Keys) {
    $rpcEnv[$k] = $commonEnv[$k]
}
$rpcEnv["NOVOVM_FUZZ_MIN_RPC_ITERS"] = "$RpcIterations"

$txLog = Join-Path $OutputDir "fuzz-min-tx-wire.log"
$rpcLog = Join-Path $OutputDir "fuzz-min-rpc-params.log"

$txResult = Invoke-TestCase `
    -CaseName "tx_wire_decode_seeded" `
    -Package "novovm-protocol" `
    -Filter "fuzz_min_tx_wire_decode_seeded_no_panic" `
    -EnvMap $txEnv `
    -LogPath $txLog `
    -TimeoutSeconds $TimeoutSec `
    -WorkingDirectory $RepoRoot

$rpcResult = Invoke-TestCase `
    -CaseName "rpc_params_seeded" `
    -Package "novovm-evm-gateway" `
    -Filter "fuzz_min_rpc_params_seeded_corpus_no_panic" `
    -EnvMap $rpcEnv `
    -LogPath $rpcLog `
    -TimeoutSeconds $TimeoutSec `
    -WorkingDirectory $RepoRoot

$allPass = [bool]($txResult.pass -and $rpcResult.pass)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    pass = $allPass
    seed = [UInt64]$Seed
    timeout_seconds = $TimeoutSec
    tx_wire_iterations = $TxWireIterations
    rpc_iterations = $RpcIterations
    fixed_corpus = [ordered]@{
        tx_wire = "embedded corpus in crates/novovm-protocol/src/tx_wire.rs::fuzz_min_tx_wire_decode_seeded_no_panic"
        rpc_params = "embedded corpus in crates/gateways/evm-gateway/src/main_tests.rs::fuzz_min_rpc_params_seeded_corpus_no_panic"
    }
    cases = @($txResult, $rpcResult)
}

$summaryJson = Join-Path $OutputDir "fuzz-min-gate-summary.json"
$summaryMd = Join-Path $OutputDir "fuzz-min-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Fuzz Min Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- pass: $($summary.pass)"
    "- seed: $($summary.seed)"
    "- timeout_seconds: $($summary.timeout_seconds)"
    "- tx_wire_iterations: $($summary.tx_wire_iterations)"
    "- rpc_iterations: $($summary.rpc_iterations)"
    "- fixed_corpus.tx_wire: $($summary.fixed_corpus.tx_wire)"
    "- fixed_corpus.rpc_params: $($summary.fixed_corpus.rpc_params)"
    ""
    "## Cases"
    ""
    "| case | package | pass | timed_out | exit_code | log |"
    "|---|---|---|---|---:|---|"
)
foreach ($case in $summary.cases) {
    $md += "| $($case.case_name) | $($case.package) | $($case.pass) | $($case.timed_out) | $($case.exit_code) | $($case.log) |"
}
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "fuzz min gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md:   $summaryMd"

if (-not $summary.pass) {
    throw "fuzz min gate FAILED"
}

Write-Host "fuzz min gate PASS"
