param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [double]$AllowedRegressionPct = -5.0,
    [ValidateRange(1, 9)]
    [int]$PerformanceRuns = 3,
    [ValidateRange(2, 20)]
    [int]$AdapterStabilityRuns = 3,
    [switch]$FullSnapshotProfileV2,
    [switch]$FullSnapshotProfileGA
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
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\release-snapshot-$dateTag"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$acceptanceScript = Join-Path $RepoRoot "scripts\migration\run_migration_acceptance_gate.ps1"
if (-not (Test-Path $acceptanceScript)) {
    throw "missing acceptance gate script: $acceptanceScript"
}

$acceptanceOutputDir = Join-Path $OutputDir "acceptance-gate-full"
if ($FullSnapshotProfileGA) {
    & $acceptanceScript `
        -RepoRoot $RepoRoot `
        -OutputDir $acceptanceOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns `
        -FullSnapshotProfileGA | Out-Null
} elseif ($FullSnapshotProfileV2) {
    & $acceptanceScript `
        -RepoRoot $RepoRoot `
        -OutputDir $acceptanceOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns `
        -FullSnapshotProfileV2 | Out-Null
} else {
    & $acceptanceScript `
        -RepoRoot $RepoRoot `
        -OutputDir $acceptanceOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns `
        -FullSnapshotProfile | Out-Null
}

$acceptanceSummaryJson = Join-Path $acceptanceOutputDir "acceptance-gate-summary.json"
if (-not (Test-Path $acceptanceSummaryJson)) {
    throw "missing acceptance summary json: $acceptanceSummaryJson"
}
$acceptance = Get-Content -Path $acceptanceSummaryJson -Raw | ConvertFrom-Json

$performanceSummaryJson = [string]$acceptance.performance_report_json
$functionalSummaryJson = [string]$acceptance.functional_report_json
$governanceRpcSummaryJson = [string]$acceptance.governance_rpc_report_json
$rpcExposureSummaryJson = [string]$acceptance.rpc_exposure_report_json

if (-not (Test-Path $performanceSummaryJson)) {
    throw "missing performance summary json: $performanceSummaryJson"
}
if (-not (Test-Path $functionalSummaryJson)) {
    throw "missing functional summary json: $functionalSummaryJson"
}
if (-not (Test-Path $governanceRpcSummaryJson)) {
    throw "missing governance rpc summary json: $governanceRpcSummaryJson"
}
if ([bool]$acceptance.rpc_exposure_gate_enabled -and -not (Test-Path $rpcExposureSummaryJson)) {
    throw "missing rpc exposure summary json: $rpcExposureSummaryJson"
}

$performance = Get-Content -Path $performanceSummaryJson -Raw | ConvertFrom-Json
$functional = Get-Content -Path $functionalSummaryJson -Raw | ConvertFrom-Json
$governanceRpc = Get-Content -Path $governanceRpcSummaryJson -Raw | ConvertFrom-Json
$rpcExposure = if ([bool]$acceptance.rpc_exposure_gate_enabled) {
    Get-Content -Path $rpcExposureSummaryJson -Raw | ConvertFrom-Json
} else {
    $null
}

$tpsP50 = [ordered]@{}
foreach ($item in $performance.compare) {
    $key = "$($item.variant)/$($item.preset)"
    $tpsP50[$key] = [double]$item.current_tps_p50
}

$enabledGates = [ordered]@{
    chain_query_rpc = [bool]$acceptance.chain_query_rpc_gate_enabled
    governance_rpc = [bool]$acceptance.governance_rpc_gate_enabled
    header_sync = [bool]$acceptance.header_sync_gate_enabled
    fast_state_sync = [bool]$acceptance.fast_state_sync_gate_enabled
    network_dos = [bool]$acceptance.network_dos_gate_enabled
    pacemaker_failover = [bool]$acceptance.pacemaker_failover_gate_enabled
    slash_governance = [bool]$acceptance.slash_governance_gate_enabled
    slash_policy_external = [bool]$acceptance.slash_policy_external_gate_enabled
    governance_hook = [bool]$acceptance.governance_hook_gate_enabled
    governance_execution = [bool]$acceptance.governance_execution_gate_enabled
    governance_param2 = [bool]$acceptance.governance_param2_gate_enabled
    governance_param3 = [bool]$acceptance.governance_param3_gate_enabled
    governance_market_policy = [bool]$acceptance.governance_market_policy_gate_enabled
    governance_council_policy = [bool]$acceptance.governance_council_policy_gate_enabled
    governance_negative = [bool]$acceptance.governance_negative_gate_enabled
    governance_access_policy = [bool]$acceptance.governance_access_policy_gate_enabled
    governance_token_economics = [bool]$acceptance.governance_token_economics_gate_enabled
    governance_treasury_spend = [bool]$acceptance.governance_treasury_spend_gate_enabled
    rpc_exposure = [bool]$acceptance.rpc_exposure_gate_enabled
    unjail_cooldown = [bool]$acceptance.unjail_cooldown_gate_enabled
    adapter_stability = [bool]$acceptance.adapter_stability_enabled
}

$governancePass = [bool](
    $acceptance.governance_rpc_pass -and
    $acceptance.governance_rpc_audit_persist_pass -and
    $acceptance.governance_rpc_signature_scheme_reject_pass -and
    $acceptance.governance_rpc_vote_verifier_startup_pass -and
    $acceptance.governance_rpc_vote_verifier_staged_reject_pass -and
    $acceptance.governance_hook_pass -and
    $acceptance.governance_execution_pass -and
    $acceptance.governance_param2_pass -and
    $acceptance.governance_param3_pass -and
    $acceptance.governance_market_policy_pass -and
    $acceptance.governance_market_policy_engine_pass -and
    $acceptance.governance_market_policy_treasury_pass -and
    $acceptance.governance_council_policy_pass -and
    $acceptance.governance_negative_pass -and
    $acceptance.governance_access_policy_pass -and
    $acceptance.governance_token_economics_pass -and
    $acceptance.governance_treasury_spend_pass
)

$syncPass = [bool](
    $acceptance.header_sync_pass -and
    $acceptance.fast_state_sync_pass -and
    $acceptance.pacemaker_failover_pass
)

$consensusPass = [bool](
    $acceptance.slash_governance_pass -and
    $acceptance.slash_policy_external_pass -and
    $acceptance.unjail_cooldown_pass -and
    $functional.consensus_negative_signal.pass
)

$keyResults = [ordered]@{
    tps_p50 = $tpsP50
    rpc_pass = [bool]$acceptance.chain_query_rpc_pass
    governance_pass = $governancePass
    sync_pass = $syncPass
    adapter_pass = [bool]$acceptance.adapter_stability_pass
    dos_pass = [bool]$acceptance.network_dos_pass
    consensus_pass = $consensusPass
    functional_pass = [bool]$acceptance.functional_pass
    performance_pass = [bool]$acceptance.performance_pass
    governance_rpc_duplicate_reject = [bool]$governanceRpc.duplicate_reject_ok
    governance_rpc_audit_persist_pass = [bool]$acceptance.governance_rpc_audit_persist_pass
    governance_rpc_signature_scheme_reject_pass = [bool]$acceptance.governance_rpc_signature_scheme_reject_pass
    governance_rpc_vote_verifier_startup_pass = [bool]$acceptance.governance_rpc_vote_verifier_startup_pass
    governance_rpc_vote_verifier_staged_reject_pass = [bool]$acceptance.governance_rpc_vote_verifier_staged_reject_pass
    governance_market_policy_pass = [bool]$acceptance.governance_market_policy_pass
    governance_market_policy_engine_pass = [bool]$acceptance.governance_market_policy_engine_pass
    governance_market_policy_treasury_pass = [bool]$acceptance.governance_market_policy_treasury_pass
    governance_council_policy_pass = [bool]$acceptance.governance_council_policy_pass
    governance_access_policy_pass = [bool]$acceptance.governance_access_policy_pass
    governance_token_economics_pass = [bool]$acceptance.governance_token_economics_pass
    governance_treasury_spend_pass = [bool]$acceptance.governance_treasury_spend_pass
    rpc_exposure_pass = if ([bool]$acceptance.rpc_exposure_gate_enabled) { [bool]$acceptance.rpc_exposure_pass } else { $true }
    rpc_exposure_default_safe_pass = if ($rpcExposure) { [bool]$rpcExposure.default_safe_pass } else { $true }
    rpc_exposure_controlled_open_pass = if ($rpcExposure) { [bool]$rpcExposure.controlled_open_pass } else { $true }
}

$snapshot = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    date = $dateTag
    profile_name = [string]$acceptance.profile_name
    overall_pass = [bool]$acceptance.overall_pass
    allowed_regression_pct = $AllowedRegressionPct
    performance_runs = $PerformanceRuns
    adapter_stability_runs = $AdapterStabilityRuns
    enabled_gates = $enabledGates
    key_results = $keyResults
    evidence = [ordered]@{
        acceptance_summary_json = $acceptanceSummaryJson
        functional_summary_json = $functionalSummaryJson
        performance_summary_json = $performanceSummaryJson
        governance_rpc_summary_json = $governanceRpcSummaryJson
        governance_market_policy_summary_json = [string]$acceptance.governance_market_policy_report_json
        governance_council_policy_summary_json = [string]$acceptance.governance_council_policy_report_json
        governance_access_policy_summary_json = [string]$acceptance.governance_access_policy_report_json
        governance_treasury_spend_summary_json = [string]$acceptance.governance_treasury_spend_report_json
        rpc_exposure_summary_json = if ([bool]$acceptance.rpc_exposure_gate_enabled) { $rpcExposureSummaryJson } else { "" }
    }
}

$snapshotJsonPath = Join-Path $OutputDir "release-snapshot.json"
$snapshotMdPath = Join-Path $OutputDir "release-snapshot.md"

$snapshot | ConvertTo-Json -Depth 12 | Set-Content -Path $snapshotJsonPath -Encoding UTF8

$md = @(
    "# NOVOVM Release Snapshot",
    "",
    "- generated_at_utc: $($snapshot.generated_at_utc)",
    "- date: $($snapshot.date)",
    "- profile_name: $($snapshot.profile_name)",
    "- overall_pass: $($snapshot.overall_pass)",
    "- allowed_regression_pct: $($snapshot.allowed_regression_pct)",
    "- performance_runs: $($snapshot.performance_runs)",
    "- adapter_stability_runs: $($snapshot.adapter_stability_runs)",
    "- rpc_pass: $($snapshot.key_results.rpc_pass)",
    "- governance_pass: $($snapshot.key_results.governance_pass)",
    "- sync_pass: $($snapshot.key_results.sync_pass)",
    "- adapter_pass: $($snapshot.key_results.adapter_pass)",
    "- dos_pass: $($snapshot.key_results.dos_pass)",
    "- consensus_pass: $($snapshot.key_results.consensus_pass)",
    "- tps_p50: $(($snapshot.key_results.tps_p50 | ConvertTo-Json -Compress))",
    "- acceptance_summary_json: $($snapshot.evidence.acceptance_summary_json)",
    "- functional_summary_json: $($snapshot.evidence.functional_summary_json)",
    "- performance_summary_json: $($snapshot.evidence.performance_summary_json)",
    "- governance_rpc_summary_json: $($snapshot.evidence.governance_rpc_summary_json)",
    "- governance_market_policy_summary_json: $($snapshot.evidence.governance_market_policy_summary_json)",
    "- governance_council_policy_summary_json: $($snapshot.evidence.governance_council_policy_summary_json)",
    "- governance_access_policy_summary_json: $($snapshot.evidence.governance_access_policy_summary_json)",
    "- governance_treasury_spend_summary_json: $($snapshot.evidence.governance_treasury_spend_summary_json)",
    "- rpc_exposure_summary_json: $($snapshot.evidence.rpc_exposure_summary_json)",
    "- snapshot_json: $snapshotJsonPath"
)
$md -join "`n" | Set-Content -Path $snapshotMdPath -Encoding UTF8

Write-Host "release snapshot generated:"
Write-Host "  overall_pass: $($snapshot.overall_pass)"
Write-Host "  profile_name: $($snapshot.profile_name)"
Write-Host "  snapshot_json: $snapshotJsonPath"
Write-Host "  snapshot_md: $snapshotMdPath"

if (-not $snapshot.overall_pass) {
    throw "release snapshot FAILED: overall_pass=false"
}

Write-Host "release snapshot PASS"
