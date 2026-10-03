param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [ValidateRange(1000, 50000000)]
    [int]$Txs = 20000,
    [ValidateRange(1000, 50000000)]
    [int]$Accounts = 5000,
    [ValidateRange(50, 1000000)]
    [int]$BatchSize = 500,
    [ValidateRange(4, 100)]
    [int]$Validators = 4,
    [ValidateRange(1, 1000000)]
    [int]$MaxBatches = 200,
    [ValidateSet("core", "persist", "wasm")]
    [string]$AoemVariant = "persist",
    [ValidateSet("inmemory", "udp_loopback")]
    [string]$NetworkTransport = "inmemory",
    [ValidateSet("auto", "ops_wire_v1", "ops_v2")]
    [string]$D1IngressMode = "auto",
    [string]$D1Codec = "",
    [ValidateSet("release", "debug")]
    [string]$BuildProfile = "release",
    [switch]$SkipBuild,
    [ValidateRange(60, 7200)]
    [int]$TimeoutSec = 1200
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}

$dateTag = Get-Date -Format "yyyy-MM-dd"
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\testnet-bootstrap-gate-$dateTag"
}
if (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
    $OutputDir = Join-Path $RepoRoot $OutputDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$e2eScript = Join-Path $RepoRoot "scripts\migration\run_consensus_network_e2e_tps.ps1"
if (-not (Test-Path $e2eScript)) {
    throw "missing dependency script: $e2eScript"
}

$e2eOutputDir = Join-Path $OutputDir "consensus-network-e2e"
New-Item -ItemType Directory -Force -Path $e2eOutputDir | Out-Null

$e2eArgs = @{
    RepoRoot = $RepoRoot
    OutputDir = $e2eOutputDir
    Txs = $Txs
    Accounts = $Accounts
    BatchSize = $BatchSize
    Validators = $Validators
    MaxBatches = $MaxBatches
    AoemVariant = $AoemVariant
    NetworkTransport = $NetworkTransport
    D1IngressMode = $D1IngressMode
    BuildProfile = $BuildProfile
    TimeoutSec = $TimeoutSec
}
if ($D1Codec -and $D1Codec.Trim().Length -gt 0) {
    $e2eArgs["D1Codec"] = $D1Codec.Trim()
}
if ($SkipBuild.IsPresent) {
    $e2eArgs["SkipBuild"] = $true
}

Write-Host "testnet bootstrap gate: running consensus network e2e ..."
$prevTargetDir = [Environment]::GetEnvironmentVariable("CARGO_TARGET_DIR", "Process")
Set-Item -Path Env:CARGO_TARGET_DIR -Value (Join-Path $RepoRoot "target")
try {
    & $e2eScript @e2eArgs
} finally {
    if ($null -eq $prevTargetDir -or $prevTargetDir -eq "") {
        Remove-Item -Path Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        Set-Item -Path Env:CARGO_TARGET_DIR -Value $prevTargetDir
    }
}

$e2eSummaryPath = Join-Path $e2eOutputDir "consensus-network-e2e-summary.json"
if (-not (Test-Path $e2eSummaryPath)) {
    throw "consensus network e2e summary missing: $e2eSummaryPath"
}

$e2eSummary = Get-Content $e2eSummaryPath -Raw | ConvertFrom-Json

$validatorsOk = [int]$e2eSummary.validators -ge 4
$batchesOk = [int]$e2eSummary.batches -gt 0
$tpsOk = [double]$e2eSummary.consensus_network_e2e_tps_p50 -gt 0
$runtimeOk = [double]$e2eSummary.runtime_total_ms -gt 0
$networkMsgOk = [double]$e2eSummary.network_message_count -gt 0
$ingressOk = -not [string]::IsNullOrWhiteSpace([string]$e2eSummary.d1_ingress_mode)

$reasons = New-Object System.Collections.Generic.List[string]
if (-not $validatorsOk) { $reasons.Add("validators<4") | Out-Null }
if (-not $batchesOk) { $reasons.Add("batches<=0") | Out-Null }
if (-not $tpsOk) { $reasons.Add("consensus_network_e2e_tps_p50<=0") | Out-Null }
if (-not $runtimeOk) { $reasons.Add("runtime_total_ms<=0") | Out-Null }
if (-not $networkMsgOk) { $reasons.Add("network_message_count<=0") | Out-Null }
if (-not $ingressOk) { $reasons.Add("d1_ingress_mode_missing") | Out-Null }

$pass = $reasons.Count -eq 0

$summary = [pscustomobject]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    profile = "testnet_bootstrap_gate_v1"
    pass = $pass
    reasons = @($reasons)
    inputs = [pscustomobject]@{
        txs = $Txs
        accounts = $Accounts
        batch_size = $BatchSize
        validators = $Validators
        max_batches = $MaxBatches
        aoem_variant = $AoemVariant
        network_transport = $NetworkTransport
        d1_ingress_mode = $D1IngressMode
        d1_codec = $D1Codec
        build_profile = $BuildProfile
    }
    checks = [pscustomobject]@{
        validators_ge_4 = $validatorsOk
        batches_gt_0 = $batchesOk
        consensus_network_e2e_tps_p50_gt_0 = $tpsOk
        runtime_total_ms_gt_0 = $runtimeOk
        network_message_count_gt_0 = $networkMsgOk
        d1_ingress_mode_present = $ingressOk
    }
    evidence = [pscustomobject]@{
        consensus_network_e2e_summary_json = $e2eSummaryPath
        consensus_network_e2e_summary_md = (Join-Path $e2eOutputDir "consensus-network-e2e-summary.md")
        consensus_network_e2e_stdout = (Join-Path $e2eOutputDir "consensus-network-e2e.stdout.log")
        consensus_network_e2e_stderr = (Join-Path $e2eOutputDir "consensus-network-e2e.stderr.log")
    }
    metrics = [pscustomobject]@{
        validators = [int]$e2eSummary.validators
        batches = [int]$e2eSummary.batches
        consensus_network_e2e_tps_p50 = [double]$e2eSummary.consensus_network_e2e_tps_p50
        consensus_network_e2e_tps_p90 = [double]$e2eSummary.consensus_network_e2e_tps_p90
        consensus_network_e2e_tps_p99 = [double]$e2eSummary.consensus_network_e2e_tps_p99
        consensus_network_e2e_latency_ms_p50 = [double]$e2eSummary.consensus_network_e2e_latency_ms_p50
        consensus_network_e2e_latency_ms_p90 = [double]$e2eSummary.consensus_network_e2e_latency_ms_p90
        consensus_network_e2e_latency_ms_p99 = [double]$e2eSummary.consensus_network_e2e_latency_ms_p99
        network_message_count = [double]$e2eSummary.network_message_count
        runtime_total_ms = [double]$e2eSummary.runtime_total_ms
        d1_ingress_mode = [string]$e2eSummary.d1_ingress_mode
        d1_input_source = [string]$e2eSummary.d1_input_source
        d1_codec = [string]$e2eSummary.d1_codec
    }
}

$summaryJson = Join-Path $OutputDir "testnet-bootstrap-gate-summary.json"
$summaryMd = Join-Path $OutputDir "testnet-bootstrap-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @()
$md += "# NOVOVM Testnet Bootstrap Gate Summary ($dateTag)"
$md += ""
$md += "- profile: $($summary.profile)"
$md += "- pass: $($summary.pass)"
$md += "- reasons: $([string]::Join(', ', $summary.reasons))"
$md += ""
$md += "## Inputs"
$md += ""
$md += "- txs: $Txs"
$md += "- accounts: $Accounts"
$md += "- batch_size: $BatchSize"
$md += "- validators: $Validators"
$md += "- max_batches: $MaxBatches"
$md += "- network_transport: $NetworkTransport"
$md += "- aoem_variant: $AoemVariant"
$md += "- d1_ingress_mode: $D1IngressMode"
$md += "- d1_codec: $D1Codec"
$md += "- build_profile: $BuildProfile"
$md += ""
$md += "## Metrics"
$md += ""
$md += "- consensus_network_e2e_tps p50/p90/p99: $($summary.metrics.consensus_network_e2e_tps_p50) / $($summary.metrics.consensus_network_e2e_tps_p90) / $($summary.metrics.consensus_network_e2e_tps_p99)"
$md += "- consensus_network_e2e_latency_ms p50/p90/p99: $($summary.metrics.consensus_network_e2e_latency_ms_p50) / $($summary.metrics.consensus_network_e2e_latency_ms_p90) / $($summary.metrics.consensus_network_e2e_latency_ms_p99)"
$md += "- validators: $($summary.metrics.validators)"
$md += "- batches: $($summary.metrics.batches)"
$md += "- network_message_count: $($summary.metrics.network_message_count)"
$md += "- runtime_total_ms: $($summary.metrics.runtime_total_ms)"
$md += "- d1_ingress_mode/source/codec: $($summary.metrics.d1_ingress_mode) / $($summary.metrics.d1_input_source) / $($summary.metrics.d1_codec)"
$md += ""
$md += "## Evidence"
$md += ""
$md += "- summary_json: $summaryJson"
$md += "- consensus_network_e2e_summary_json: $($summary.evidence.consensus_network_e2e_summary_json)"
$md += "- consensus_network_e2e_summary_md: $($summary.evidence.consensus_network_e2e_summary_md)"
$md += "- consensus_network_e2e_stdout: $($summary.evidence.consensus_network_e2e_stdout)"
$md += "- consensus_network_e2e_stderr: $($summary.evidence.consensus_network_e2e_stderr)"
$md += ""
$md += "## Reproduce"
$md += ""
$md += '```powershell'
$md += ('& scripts/migration/run_testnet_bootstrap_gate.ps1 -RepoRoot "{0}" -Txs {1} -Accounts {2} -BatchSize {3} -Validators {4} -MaxBatches {5} -AoemVariant {6} -NetworkTransport {7} -D1IngressMode {8} -D1Codec ''{9}'' -BuildProfile {10}' -f $RepoRoot, $Txs, $Accounts, $BatchSize, $Validators, $MaxBatches, $AoemVariant, $NetworkTransport, $D1IngressMode, $D1Codec, $BuildProfile)
$md += '```'
$md | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "testnet bootstrap gate summary:"
Write-Host "  pass: $($summary.pass)"
Write-Host "  reasons: $([string]::Join(', ', $summary.reasons))"
Write-Host "  summary_json: $summaryJson"
Write-Host "  summary_md:   $summaryMd"

if (-not $pass) {
    throw "testnet bootstrap gate FAILED: $([string]::Join(', ', $summary.reasons))"
}

Write-Host "testnet bootstrap gate PASS"
