param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "",
    [UInt64]$ChainId = 1,
    [UInt64]$AutopilotDurationMinutes = 2,
    [UInt64]$ExecproofDurationMinutes = 2,
    [UInt64]$IntervalSeconds = 5,
    [UInt64]$WarmupSeconds = 6,
    [switch]$SkipBuild,
    [switch]$FreshRlpxProfile,
    [switch]$EnableSwapPriority = $true,
    [string]$SummaryOut = "artifacts/migration/evm-full-lifecycle-autopilot-summary.json"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
    }
    return (Resolve-Path $Root).Path
}

function Resolve-FullPath {
    param(
        [string]$Root,
        [string]$Value
    )
    if ([System.IO.Path]::IsPathRooted($Value)) {
        return [System.IO.Path]::GetFullPath($Value)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $Root $Value))
}

function Read-JsonFile {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        throw "missing json file: $Path"
    }
    return (Get-Content -Path $Path -Raw | ConvertFrom-Json)
}

function Get-FreeTcpPort {
    $listener = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return [int]($listener.LocalEndpoint.Port)
    } finally {
        $listener.Stop()
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
Set-Location $RepoRoot

$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$SummaryDir = Split-Path -Parent $SummaryOut
if ($SummaryDir) {
    New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
}

if ([string]::IsNullOrWhiteSpace($GatewayBind)) {
    $freePort = Get-FreeTcpPort
    $GatewayBind = "127.0.0.1:$freePort"
}
$GatewayUrl = "http://$GatewayBind"

$autopilotSummary = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/tmp-autopilot-summary.json"
$execProofSummary = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/tmp-step2-execproof.json"
$observeSummary = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/tmp-step2-observe-summary.json"
$progressSummary = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/tmp-full-lifecycle-closure-progress.json"

$autopilotArgs = @(
    "-ExecutionPolicy", "Bypass",
    "-File", "scripts/migration/run_evm_mempool_autopilot.ps1",
    "-GatewayBind", $GatewayBind,
    "-ChainId", ([string][UInt64]$ChainId),
    "-DurationMinutes", ([string][UInt64]$AutopilotDurationMinutes),
    "-IntervalSeconds", ([string][UInt64]$IntervalSeconds),
    "-WarmupSeconds", ([string][UInt64]$WarmupSeconds),
    "-SummaryOut", $autopilotSummary
)
if ($SkipBuild) {
    $autopilotArgs += "-SkipBuild"
}
if ($FreshRlpxProfile) {
    $autopilotArgs += "-FreshRlpxProfile"
}
if ($EnableSwapPriority) {
    $autopilotArgs += "-EnableSwapPriority"
}

Write-Host "[full-lifecycle] step1: running mempool autopilot"
& powershell @autopilotArgs

$execArgs = @(
    "-ExecutionPolicy", "Bypass",
    "-File", "scripts/migration/tmp_run_step2_execproof.ps1",
    "-GatewayBind", $GatewayBind,
    "-GatewayUrl", $GatewayUrl,
    "-ChainId", ([string][UInt64]$ChainId),
    "-DurationMinutes", ([string][UInt64]$ExecproofDurationMinutes)
)
if ($SkipBuild) {
    $execArgs += "-SkipBuild"
}
Write-Host "[full-lifecycle] step2/3/4: running exec proof"
& powershell @execArgs

$auto = Read-JsonFile -Path $autopilotSummary
$exec = Read-JsonFile -Path $execProofSummary
$obs = Read-JsonFile -Path $observeSummary

$autoBest = $auto.best_profile
$bestSummaryPath = [string]$autoBest.summary
$bestObs = $null
if (-not [string]::IsNullOrWhiteSpace($bestSummaryPath) -and (Test-Path $bestSummaryPath)) {
    $bestObs = Read-JsonFile -Path $bestSummaryPath
}

$step1 = ($bestObs -ne $null) -and [bool]$bestObs.smoke.passed -and ([UInt64]$bestObs.smoke.observed_ready -gt 0) -and ([UInt64]$bestObs.smoke.observed_new_pooled -gt 0) -and ([UInt64]$bestObs.smoke.observed_pooled -gt 0)
$step2 = [bool]$exec.apply.verified -and [bool]$exec.apply.applied
$step3 = [bool]$exec.local_exec_sealed -and ([string]$exec.local_exec_head_before -ne [string]$exec.local_exec_head_after)
$step4 = [bool]$exec.proof_block_query_ok -and [bool]$exec.proof_receipt_query_ok

$uniTotal = 0
if ($obs -ne $null -and $obs.aggregate -ne $null -and $obs.aggregate.PSObject.Properties.Name -contains "max_uniswap_total") {
    $uniTotal = [int]$obs.aggregate.max_uniswap_total
}
$step5 = ($uniTotal -gt 0)

$passes = @($step1, $step2, $step3, $step4, $step5) | Where-Object { $_ -eq $true }
$progressPct = [Math]::Round(($passes.Count / 5.0) * 100.0, 2)

$result = [ordered]@{
    generated_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    chain_id = [UInt64]$ChainId
    config = [ordered]@{
        gateway_bind = $GatewayBind
        autopilot_duration_minutes = [UInt64]$AutopilotDurationMinutes
        execproof_duration_minutes = [UInt64]$ExecproofDurationMinutes
        interval_seconds = [UInt64]$IntervalSeconds
        warmup_seconds = [UInt64]$WarmupSeconds
        fresh_rlpx_profile = [bool]$FreshRlpxProfile
        enable_swap_priority = [bool]$EnableSwapPriority
        skip_build = [bool]$SkipBuild
    }
    evidence_paths = [ordered]@{
        autopilot_summary = $autopilotSummary
        autopilot_best_summary = $bestSummaryPath
        observe_summary = $observeSummary
        execproof_summary = $execProofSummary
        closure_progress = $progressSummary
    }
    step1_public_to_txpool = [ordered]@{
        pass = $step1
        smoke_passed = if ($bestObs) { [bool]$bestObs.smoke.passed } else { $false }
        observed_ready = if ($bestObs) { [UInt64]$bestObs.smoke.observed_ready } else { [UInt64]0 }
        observed_new_pooled = if ($bestObs) { [UInt64]$bestObs.smoke.observed_new_pooled } else { [UInt64]0 }
        observed_pooled = if ($bestObs) { [UInt64]$bestObs.smoke.observed_pooled } else { [UInt64]0 }
    }
    step2_txpool_to_exec = [ordered]@{
        pass = $step2
        apply_verified = [bool]$exec.apply.verified
        apply_applied = [bool]$exec.apply.applied
        indexed_txs = [UInt64]$exec.local_exec_indexed_txs
    }
    step3_exec_to_blockchain = [ordered]@{
        pass = $step3
        head_before = [string]$exec.local_exec_head_before
        head_after = [string]$exec.local_exec_head_after
        local_exec_sealed = [bool]$exec.local_exec_sealed
        local_exec_block_hash = [string]$exec.local_exec_block_hash
    }
    step4_post_chain_visibility = [ordered]@{
        pass = $step4
        proof_block_query_ok = [bool]$exec.proof_block_query_ok
        proof_receipt_query_ok = [bool]$exec.proof_receipt_query_ok
        proof_receipt_status = [string]$exec.proof_receipt_status
    }
    step5_feature_sample = [ordered]@{
        pass = $step5
        max_uniswap_total = $uniTotal
    }
    completed_steps = $passes.Count
    total_steps = 5
    progress_percent = $progressPct
    overall_pass = ($passes.Count -eq 5)
}

$result | ConvertTo-Json -Depth 64 | Set-Content -Path $SummaryOut -Encoding UTF8
Write-Host ("[full-lifecycle] summary written: {0}" -f $SummaryOut)
Write-Host ("[full-lifecycle] progress={0}% overall_pass={1}" -f $progressPct, $result.overall_pass)
