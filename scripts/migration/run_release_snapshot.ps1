param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [double]$AllowedRegressionPct = -5.0,
    [ValidateRange(1, 9)]
    [int]$PerformanceRuns = 3,
    [ValidateRange(2, 20)]
    [int]$AdapterStabilityRuns = 3,
    [switch]$IncludeGovernanceRpcMldsaFfiGate,
    [string]$GovernanceRpcMldsaFfiAoemRoot = "",
    [string]$GovernanceRpcMldsaFfiBind = "127.0.0.1:8902",
    [ValidateRange(1, 64)]
    [int]$GovernanceRpcMldsaFfiExpectedRequests = 9,
    [switch]$IncludeUnifiedAccountGate,
    [string]$AoemPluginDir = "",
    [bool]$PreferComposedAoemRuntime = $true,
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
        -AoemPluginDir $AoemPluginDir `
        -PreferComposedAoemRuntime:$PreferComposedAoemRuntime `
        -IncludeGovernanceRpcMldsaFfiGate:$IncludeGovernanceRpcMldsaFfiGate `
        -GovernanceRpcMldsaFfiAoemRoot $GovernanceRpcMldsaFfiAoemRoot `
        -GovernanceRpcMldsaFfiBind $GovernanceRpcMldsaFfiBind `
        -GovernanceRpcMldsaFfiExpectedRequests $GovernanceRpcMldsaFfiExpectedRequests `
        -IncludeUnifiedAccountGate:$IncludeUnifiedAccountGate `
        -FullSnapshotProfileGA | Out-Null
} elseif ($FullSnapshotProfileV2) {
    & $acceptanceScript `
        -RepoRoot $RepoRoot `
        -OutputDir $acceptanceOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns `
        -AoemPluginDir $AoemPluginDir `
        -PreferComposedAoemRuntime:$PreferComposedAoemRuntime `
        -IncludeGovernanceRpcMldsaFfiGate:$IncludeGovernanceRpcMldsaFfiGate `
        -GovernanceRpcMldsaFfiAoemRoot $GovernanceRpcMldsaFfiAoemRoot `
        -GovernanceRpcMldsaFfiBind $GovernanceRpcMldsaFfiBind `
        -GovernanceRpcMldsaFfiExpectedRequests $GovernanceRpcMldsaFfiExpectedRequests `
        -IncludeUnifiedAccountGate:$IncludeUnifiedAccountGate `
        -FullSnapshotProfileV2 | Out-Null
} else {
    & $acceptanceScript `
        -RepoRoot $RepoRoot `
        -OutputDir $acceptanceOutputDir `
        -AllowedRegressionPct $AllowedRegressionPct `
        -PerformanceRuns $PerformanceRuns `
        -AdapterStabilityRuns $AdapterStabilityRuns `
        -AoemPluginDir $AoemPluginDir `
        -PreferComposedAoemRuntime:$PreferComposedAoemRuntime `
        -IncludeGovernanceRpcMldsaFfiGate:$IncludeGovernanceRpcMldsaFfiGate `
        -GovernanceRpcMldsaFfiAoemRoot $GovernanceRpcMldsaFfiAoemRoot `
        -GovernanceRpcMldsaFfiBind $GovernanceRpcMldsaFfiBind `
        -GovernanceRpcMldsaFfiExpectedRequests $GovernanceRpcMldsaFfiExpectedRequests `
        -IncludeUnifiedAccountGate:$IncludeUnifiedAccountGate `
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
$governanceRpcMldsaFfiSummaryJson = [string]$acceptance.governance_rpc_mldsa_ffi_report_json
$unifiedAccountSummaryJson = [string]$acceptance.unified_account_report_json
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
if ([bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled -and -not (Test-Path $governanceRpcMldsaFfiSummaryJson)) {
    throw "missing governance rpc mldsa ffi summary json: $governanceRpcMldsaFfiSummaryJson"
}
if ([bool]$acceptance.unified_account_gate_enabled -and -not (Test-Path $unifiedAccountSummaryJson)) {
    throw "missing unified account summary json: $unifiedAccountSummaryJson"
}
if ([bool]$acceptance.rpc_exposure_gate_enabled -and -not (Test-Path $rpcExposureSummaryJson)) {
    throw "missing rpc exposure summary json: $rpcExposureSummaryJson"
}

$performance = Get-Content -Path $performanceSummaryJson -Raw | ConvertFrom-Json
$functional = Get-Content -Path $functionalSummaryJson -Raw | ConvertFrom-Json
$governanceRpc = Get-Content -Path $governanceRpcSummaryJson -Raw | ConvertFrom-Json
$governanceRpcMldsaFfi = if ([bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled) {
    Get-Content -Path $governanceRpcMldsaFfiSummaryJson -Raw | ConvertFrom-Json
} else {
    $null
}
$unifiedAccount = if ([bool]$acceptance.unified_account_gate_enabled) {
    Get-Content -Path $unifiedAccountSummaryJson -Raw | ConvertFrom-Json
} else {
    $null
}
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
    economic_infra_dedicated = [bool]$acceptance.economic_infra_dedicated_gate_enabled
    market_engine_treasury_negative = [bool]$acceptance.market_engine_treasury_negative_gate_enabled
    foreign_rate_source = [bool]$acceptance.foreign_rate_source_gate_enabled
    nav_valuation_source = [bool]$acceptance.nav_valuation_source_gate_enabled
    dividend_balance_source = [bool]$acceptance.dividend_balance_source_gate_enabled
    governance_rpc_mldsa_ffi = [bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled
    unified_account = [bool]$acceptance.unified_account_gate_enabled
    rpc_exposure = [bool]$acceptance.rpc_exposure_gate_enabled
    unjail_cooldown = [bool]$acceptance.unjail_cooldown_gate_enabled
    adapter_stability = [bool]$acceptance.adapter_stability_enabled
    vm_runtime_split = [bool]$acceptance.vm_runtime_split_gate_enabled
    evm_chain_profile_signal = [bool]$acceptance.evm_chain_profile_signal_gate_enabled
    evm_tx_type_signal = [bool]$acceptance.evm_tx_type_signal_gate_enabled
    overlap_router_signal = [bool]$acceptance.overlap_router_signal_gate_enabled
    evm_backend_compare = [bool]$acceptance.evm_backend_compare_gate_enabled
}

$governanceMldsaPass = if ([bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled) {
    [bool]($acceptance.governance_rpc_mldsa_ffi_pass -and $acceptance.governance_rpc_mldsa_ffi_startup_pass)
} else {
    $true
}

$governancePass = [bool](
    $acceptance.governance_rpc_pass -and
    $acceptance.governance_rpc_audit_persist_pass -and
    $acceptance.governance_rpc_signature_scheme_reject_pass -and
    $acceptance.governance_rpc_vote_verifier_startup_pass -and
    $acceptance.governance_rpc_vote_verifier_staged_reject_pass -and
    $acceptance.governance_rpc_vote_verifier_execute_pass -and
    $acceptance.governance_rpc_chain_audit_pass -and
    $acceptance.governance_rpc_chain_audit_execute_verifier_proof_pass -and
    $acceptance.governance_rpc_chain_audit_root_proof_pass -and
    $governanceMldsaPass -and
    $acceptance.governance_hook_pass -and
    $acceptance.governance_execution_pass -and
    $acceptance.governance_param2_pass -and
    $acceptance.governance_param3_pass -and
    $acceptance.governance_market_policy_pass -and
    $acceptance.governance_market_policy_engine_pass -and
    $acceptance.governance_market_policy_treasury_pass -and
    $acceptance.governance_market_policy_orchestration_pass -and
    $acceptance.governance_market_policy_dividend_pass -and
    $acceptance.governance_market_policy_foreign_payment_pass -and
    $acceptance.governance_council_policy_pass -and
    $acceptance.governance_negative_pass -and
    $acceptance.governance_access_policy_pass -and
    $acceptance.governance_token_economics_pass -and
    $acceptance.governance_treasury_spend_pass
)

$economicPass = [bool](
    ((-not [bool]$acceptance.economic_infra_dedicated_gate_enabled) -or [bool]$acceptance.economic_infra_dedicated_pass) -and
    ((-not [bool]$acceptance.market_engine_treasury_negative_gate_enabled) -or [bool]$acceptance.market_engine_treasury_negative_pass) -and
    ((-not [bool]$acceptance.foreign_rate_source_gate_enabled) -or [bool]$acceptance.foreign_rate_source_pass) -and
    ((-not [bool]$acceptance.nav_valuation_source_gate_enabled) -or [bool]$acceptance.nav_valuation_source_pass) -and
    ((-not [bool]$acceptance.dividend_balance_source_gate_enabled) -or [bool]$acceptance.dividend_balance_source_pass)
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
    governance_chain_audit_root_parity_pass = [bool]$acceptance.governance_chain_audit_root_parity_pass
    performance_pass = [bool]$acceptance.performance_pass
    governance_rpc_duplicate_reject = [bool]$governanceRpc.duplicate_reject_ok
    governance_rpc_audit_persist_pass = [bool]$acceptance.governance_rpc_audit_persist_pass
    governance_rpc_signature_scheme_reject_pass = [bool]$acceptance.governance_rpc_signature_scheme_reject_pass
    governance_rpc_vote_verifier_startup_pass = [bool]$acceptance.governance_rpc_vote_verifier_startup_pass
    governance_rpc_vote_verifier_staged_reject_pass = [bool]$acceptance.governance_rpc_vote_verifier_staged_reject_pass
    governance_rpc_vote_verifier_execute_pass = [bool]$acceptance.governance_rpc_vote_verifier_execute_pass
    governance_rpc_chain_audit_pass = [bool]$acceptance.governance_rpc_chain_audit_pass
    governance_rpc_chain_audit_persist_pass = [bool]$acceptance.governance_rpc_chain_audit_persist_pass
    governance_rpc_chain_audit_restart_pass = [bool]$acceptance.governance_rpc_chain_audit_restart_pass
    governance_rpc_chain_audit_execute_verifier_pass = [bool]$acceptance.governance_rpc_chain_audit_execute_verifier_pass
    governance_rpc_chain_audit_persist_execute_verifier_pass = [bool]$acceptance.governance_rpc_chain_audit_persist_execute_verifier_pass
    governance_rpc_chain_audit_restart_execute_verifier_pass = [bool]$acceptance.governance_rpc_chain_audit_restart_execute_verifier_pass
    governance_rpc_chain_audit_execute_verifier_proof_pass = [bool]$acceptance.governance_rpc_chain_audit_execute_verifier_proof_pass
    governance_rpc_chain_audit_root_proof_pass = [bool]$acceptance.governance_rpc_chain_audit_root_proof_pass
    governance_rpc_mldsa_ffi_gate_enabled = [bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled
    governance_rpc_mldsa_ffi_pass = if ([bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled) { [bool]$acceptance.governance_rpc_mldsa_ffi_pass } else { $true }
    governance_rpc_mldsa_ffi_startup_pass = if ([bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled) { [bool]$acceptance.governance_rpc_mldsa_ffi_startup_pass } else { $true }
    governance_rpc_mldsa_ffi_verify_pass = if ($governanceRpcMldsaFfi) {
        if ($governanceRpcMldsaFfi.PSObject.Properties.Name -contains "vote_verifier_startup_ok") {
            [bool]$governanceRpcMldsaFfi.vote_verifier_startup_ok
        } elseif ($governanceRpcMldsaFfi.PSObject.Properties.Name -contains "pass") {
            [bool]$governanceRpcMldsaFfi.pass
        } else {
            $false
        }
    } else {
        $true
    }
    governance_market_policy_pass = [bool]$acceptance.governance_market_policy_pass
    governance_market_policy_engine_pass = [bool]$acceptance.governance_market_policy_engine_pass
    governance_market_policy_treasury_pass = [bool]$acceptance.governance_market_policy_treasury_pass
    governance_market_policy_orchestration_pass = [bool]$acceptance.governance_market_policy_orchestration_pass
    governance_market_policy_dividend_pass = [bool]$acceptance.governance_market_policy_dividend_pass
    governance_market_policy_foreign_payment_pass = [bool]$acceptance.governance_market_policy_foreign_payment_pass
    governance_council_policy_pass = [bool]$acceptance.governance_council_policy_pass
    governance_access_policy_pass = [bool]$acceptance.governance_access_policy_pass
    governance_token_economics_pass = [bool]$acceptance.governance_token_economics_pass
    governance_treasury_spend_pass = [bool]$acceptance.governance_treasury_spend_pass
    economic_pass = $economicPass
    economic_infra_dedicated_pass = if ([bool]$acceptance.economic_infra_dedicated_gate_enabled) { [bool]$acceptance.economic_infra_dedicated_pass } else { $true }
    market_engine_treasury_negative_pass = if ([bool]$acceptance.market_engine_treasury_negative_gate_enabled) { [bool]$acceptance.market_engine_treasury_negative_pass } else { $true }
    foreign_rate_source_pass = if ([bool]$acceptance.foreign_rate_source_gate_enabled) { [bool]$acceptance.foreign_rate_source_pass } else { $true }
    nav_valuation_source_pass = if ([bool]$acceptance.nav_valuation_source_gate_enabled) { [bool]$acceptance.nav_valuation_source_pass } else { $true }
    dividend_balance_source_pass = if ([bool]$acceptance.dividend_balance_source_gate_enabled) { [bool]$acceptance.dividend_balance_source_pass } else { $true }
    unified_account_gate_enabled = [bool]$acceptance.unified_account_gate_enabled
    unified_account_pass = if ([bool]$acceptance.unified_account_gate_enabled) { [bool]$acceptance.unified_account_pass } else { $true }
    unified_account_block_merge_pass = if ($unifiedAccount) { [bool]$unifiedAccount.block_merge_pass } else { $true }
    unified_account_block_release_pass = if ($unifiedAccount) { [bool]$unifiedAccount.block_release_pass } else { $true }
    rpc_exposure_pass = if ([bool]$acceptance.rpc_exposure_gate_enabled) { [bool]$acceptance.rpc_exposure_pass } else { $true }
    rpc_exposure_default_safe_pass = if ($rpcExposure) { [bool]$rpcExposure.default_safe_pass } else { $true }
    rpc_exposure_controlled_open_pass = if ($rpcExposure) { [bool]$rpcExposure.controlled_open_pass } else { $true }
    evm_chain_profile_signal_pass = if ([bool]$acceptance.evm_chain_profile_signal_gate_enabled) { [bool]$acceptance.evm_chain_profile_signal_pass } else { $true }
    evm_tx_type_signal_pass = if ([bool]$acceptance.evm_tx_type_signal_gate_enabled) { [bool]$acceptance.evm_tx_type_signal_pass } else { $true }
    overlap_router_signal_pass = if ([bool]$acceptance.overlap_router_signal_gate_enabled) { [bool]$acceptance.overlap_router_signal_pass } else { $true }
    evm_backend_compare_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_pass } else { $true }
    evm_backend_compare_evm_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_evm_pass } else { $true }
    evm_backend_compare_polygon_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_polygon_pass } else { $true }
    evm_backend_compare_bnb_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_bnb_pass } else { $true }
    evm_backend_compare_avalanche_pass = if ([bool]$acceptance.evm_backend_compare_gate_enabled) { [bool]$acceptance.evm_backend_compare_avalanche_pass } else { $true }
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
        governance_rpc_mldsa_ffi_summary_json = if ([bool]$acceptance.governance_rpc_mldsa_ffi_gate_enabled) { $governanceRpcMldsaFfiSummaryJson } else { "" }
        governance_market_policy_summary_json = [string]$acceptance.governance_market_policy_report_json
        governance_council_policy_summary_json = [string]$acceptance.governance_council_policy_report_json
        governance_access_policy_summary_json = [string]$acceptance.governance_access_policy_report_json
        governance_treasury_spend_summary_json = [string]$acceptance.governance_treasury_spend_report_json
        economic_infra_dedicated_summary_json = [string]$acceptance.economic_infra_dedicated_report_json
        market_engine_treasury_negative_summary_json = [string]$acceptance.market_engine_treasury_negative_report_json
        foreign_rate_source_summary_json = [string]$acceptance.foreign_rate_source_report_json
        nav_valuation_source_summary_json = [string]$acceptance.nav_valuation_source_report_json
        dividend_balance_source_summary_json = [string]$acceptance.dividend_balance_source_report_json
        unified_account_summary_json = if ([bool]$acceptance.unified_account_gate_enabled) { $unifiedAccountSummaryJson } else { "" }
        rpc_exposure_summary_json = if ([bool]$acceptance.rpc_exposure_gate_enabled) { $rpcExposureSummaryJson } else { "" }
        evm_chain_profile_signal_summary_json = [string]$acceptance.evm_chain_profile_signal_report_json
        evm_tx_type_signal_summary_json = [string]$acceptance.evm_tx_type_signal_report_json
        overlap_router_signal_summary_json = [string]$acceptance.overlap_router_signal_report_json
        evm_backend_compare_evm_summary_json = [string]$acceptance.evm_backend_compare_evm_report_json
        evm_backend_compare_polygon_summary_json = [string]$acceptance.evm_backend_compare_polygon_report_json
        evm_backend_compare_bnb_summary_json = [string]$acceptance.evm_backend_compare_bnb_report_json
        evm_backend_compare_avalanche_summary_json = [string]$acceptance.evm_backend_compare_avalanche_report_json
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
    "- governance_chain_audit_root_parity_pass: $($snapshot.key_results.governance_chain_audit_root_parity_pass)",
    "- evm_chain_profile_signal_pass: $($snapshot.key_results.evm_chain_profile_signal_pass)",
    "- evm_tx_type_signal_pass: $($snapshot.key_results.evm_tx_type_signal_pass)",
    "- overlap_router_signal_pass: $($snapshot.key_results.overlap_router_signal_pass)",
    "- evm_backend_compare_pass: $($snapshot.key_results.evm_backend_compare_pass)",
    "- evm_backend_compare_evm_pass: $($snapshot.key_results.evm_backend_compare_evm_pass)",
    "- evm_backend_compare_polygon_pass: $($snapshot.key_results.evm_backend_compare_polygon_pass)",
    "- evm_backend_compare_bnb_pass: $($snapshot.key_results.evm_backend_compare_bnb_pass)",
    "- evm_backend_compare_avalanche_pass: $($snapshot.key_results.evm_backend_compare_avalanche_pass)",
    "- tps_p50: $(($snapshot.key_results.tps_p50 | ConvertTo-Json -Compress))",
    "- acceptance_summary_json: $($snapshot.evidence.acceptance_summary_json)",
    "- functional_summary_json: $($snapshot.evidence.functional_summary_json)",
    "- performance_summary_json: $($snapshot.evidence.performance_summary_json)",
    "- governance_rpc_summary_json: $($snapshot.evidence.governance_rpc_summary_json)",
    "- governance_rpc_mldsa_ffi_summary_json: $($snapshot.evidence.governance_rpc_mldsa_ffi_summary_json)",
    "- governance_market_policy_summary_json: $($snapshot.evidence.governance_market_policy_summary_json)",
    "- governance_council_policy_summary_json: $($snapshot.evidence.governance_council_policy_summary_json)",
    "- governance_access_policy_summary_json: $($snapshot.evidence.governance_access_policy_summary_json)",
    "- governance_treasury_spend_summary_json: $($snapshot.evidence.governance_treasury_spend_summary_json)",
    "- economic_infra_dedicated_summary_json: $($snapshot.evidence.economic_infra_dedicated_summary_json)",
    "- market_engine_treasury_negative_summary_json: $($snapshot.evidence.market_engine_treasury_negative_summary_json)",
    "- foreign_rate_source_summary_json: $($snapshot.evidence.foreign_rate_source_summary_json)",
    "- nav_valuation_source_summary_json: $($snapshot.evidence.nav_valuation_source_summary_json)",
    "- dividend_balance_source_summary_json: $($snapshot.evidence.dividend_balance_source_summary_json)",
    "- unified_account_summary_json: $($snapshot.evidence.unified_account_summary_json)",
    "- rpc_exposure_summary_json: $($snapshot.evidence.rpc_exposure_summary_json)",
    "- evm_chain_profile_signal_summary_json: $($snapshot.evidence.evm_chain_profile_signal_summary_json)",
    "- evm_tx_type_signal_summary_json: $($snapshot.evidence.evm_tx_type_signal_summary_json)",
    "- overlap_router_signal_summary_json: $($snapshot.evidence.overlap_router_signal_summary_json)",
    "- evm_backend_compare_evm_summary_json: $($snapshot.evidence.evm_backend_compare_evm_summary_json)",
    "- evm_backend_compare_polygon_summary_json: $($snapshot.evidence.evm_backend_compare_polygon_summary_json)",
    "- evm_backend_compare_bnb_summary_json: $($snapshot.evidence.evm_backend_compare_bnb_summary_json)",
    "- evm_backend_compare_avalanche_summary_json: $($snapshot.evidence.evm_backend_compare_avalanche_summary_json)",
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
