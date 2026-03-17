param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [double]$AllowedRegressionPct = -5.0,
    [ValidateRange(1, 9)]
    [int]$PerformanceRuns = 3,
    [ValidateRange(0, 3)]
    [int]$PerformanceBorderlineRetries = 2,
    [ValidateRange(0.0, 2.0)]
    [double]$PerformanceBorderlineEpsilonPct = 0.2,
    [switch]$FullSnapshotProfile,
    [switch]$FullSnapshotProfileV2,
    [switch]$FullSnapshotProfileGA,
    [bool]$IncludePerformanceGate = $true,
    [bool]$IncludeChainQueryRpcGate = $true,
    [string]$ChainQueryRpcBind = "127.0.0.1:8899",
    [ValidateRange(1, 32)]
    [int]$ChainQueryRpcExpectedRequests = 5,
    [bool]$IncludeGovernanceRpcGate = $true,
    [string]$GovernanceRpcBind = "127.0.0.1:8901",
    [ValidateRange(1, 64)]
    [int]$GovernanceRpcExpectedRequests = 16,
    [bool]$IncludeGovernanceRpcMldsaFfiGate = $false,
    [string]$GovernanceRpcMldsaFfiBind = "127.0.0.1:8902",
    [ValidateRange(1, 64)]
    [int]$GovernanceRpcMldsaFfiExpectedRequests = 9,
    [string]$GovernanceRpcMldsaFfiAoemRoot = "",
    [bool]$IncludeHeaderSyncGate = $true,
    [bool]$IncludeFastStateSyncGate = $true,
    [bool]$IncludeNetworkDosGate = $true,
    [bool]$IncludePacemakerFailoverGate = $true,
    [bool]$IncludeSlashGovernanceGate = $true,
    [bool]$IncludeSlashPolicyExternalGate = $true,
    [bool]$IncludeGovernanceHookGate = $true,
    [bool]$IncludeGovernanceExecutionGate = $true,
    [bool]$IncludeGovernanceParam2Gate = $true,
    [bool]$IncludeGovernanceParam3Gate = $true,
    [bool]$IncludeGovernanceMarketPolicyGate = $false,
    [bool]$IncludeGovernanceCouncilPolicyGate = $false,
    [bool]$IncludeGovernanceNegativeGate = $true,
    [bool]$IncludeGovernanceAccessPolicyGate = $false,
    [bool]$IncludeGovernanceTokenEconomicsGate = $false,
    [bool]$IncludeGovernanceTreasurySpendGate = $false,
    [bool]$IncludeEconomicInfraDedicatedGate = $false,
    [bool]$IncludeEconomicServiceSurfaceGate = $false,
    [bool]$IncludeOpsControlSurfaceGate = $false,
    [bool]$IncludeMarketEngineTreasuryNegativeGate = $false,
    [bool]$IncludeForeignRateSourceGate = $false,
    [bool]$IncludeNavValuationSourceGate = $false,
    [bool]$IncludeDividendBalanceSourceGate = $false,
    [bool]$IncludeUnifiedAccountGate = $false,
    [bool]$IncludeRpcExposureGate = $false,
    [bool]$IncludeTestnetBootstrapGate = $false,
    [string]$RpcExposurePublicBind = "127.0.0.1:8899",
    [string]$RpcExposureGovBind = "127.0.0.1:8901",
    [bool]$IncludeUnjailCooldownGate = $true,
    [ValidateRange(4, 1000)]
    [int]$PacemakerFailoverNodes = 4,
    [ValidateRange(0, 999)]
    [int]$PacemakerFailoverFailedLeader = 0,
    [bool]$IncludeAdapterStabilityGate = $true,
    [bool]$IncludeVmRuntimeSplitGate = $true,
    [bool]$IncludeEvmChainProfileSignalGate = $true,
    [bool]$IncludeEvmTxTypeSignalGate = $true,
    [bool]$IncludeOverlapRouterSignalGate = $true,
    [bool]$IncludeEvmBackendCompareGate = $true,
    [bool]$EvmBackendCompareIncludeBnb = $true,
    [bool]$EvmBackendCompareIncludePolygon = $true,
    [bool]$EvmBackendCompareIncludeAvalanche = $true,
    [ValidateRange(2, 20)]
    [int]$AdapterStabilityRuns = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$profileName = "default"
if ($FullSnapshotProfile -or $FullSnapshotProfileV2 -or $FullSnapshotProfileGA) {
    $IncludeChainQueryRpcGate = $false
    $IncludeGovernanceRpcGate = $false
    $IncludeHeaderSyncGate = $false
    $IncludeFastStateSyncGate = $false
    $IncludeNetworkDosGate = $false
    $IncludePacemakerFailoverGate = $false
    $IncludeSlashGovernanceGate = $false
    $IncludeSlashPolicyExternalGate = $false
    $IncludeGovernanceHookGate = $false
    $IncludeGovernanceExecutionGate = $false
    $IncludeGovernanceParam2Gate = $false
    $IncludeGovernanceParam3Gate = $false
    $IncludeGovernanceMarketPolicyGate = $false
    $IncludeGovernanceCouncilPolicyGate = $false
    $IncludeGovernanceNegativeGate = $false
    $IncludeGovernanceAccessPolicyGate = $false
    $IncludeUnjailCooldownGate = $false
    $IncludeAdapterStabilityGate = $false
    $IncludeVmRuntimeSplitGate = $false
    $IncludeEvmChainProfileSignalGate = $false
    $IncludeEvmTxTypeSignalGate = $false
    $IncludeOverlapRouterSignalGate = $false
    $IncludeEvmBackendCompareGate = $false
    $EvmBackendCompareIncludeBnb = $true
    $EvmBackendCompareIncludePolygon = $true
    $EvmBackendCompareIncludeAvalanche = $true
    $IncludeGovernanceTokenEconomicsGate = $false
    $IncludeGovernanceTreasurySpendGate = $false
    $IncludeEconomicInfraDedicatedGate = $false
    $IncludeEconomicServiceSurfaceGate = $false
    $IncludeOpsControlSurfaceGate = $false
    $IncludeMarketEngineTreasuryNegativeGate = $false
    $IncludeForeignRateSourceGate = $false
    $IncludeNavValuationSourceGate = $false
    $IncludeDividendBalanceSourceGate = $false
    $IncludeUnifiedAccountGate = $false
    $IncludeRpcExposureGate = $false
    $profileName = "full_snapshot_v1"
}
if ($FullSnapshotProfileV2 -or $FullSnapshotProfileGA) {
    $IncludeRpcExposureGate = $false
    $profileName = "full_snapshot_v2"
}
if ($FullSnapshotProfileGA) {
    $IncludeGovernanceAccessPolicyGate = $false
    $IncludeGovernanceTokenEconomicsGate = $false
    $IncludeGovernanceTreasurySpendGate = $false
    $IncludeGovernanceMarketPolicyGate = $false
    $IncludeGovernanceCouncilPolicyGate = $false
    $IncludeEconomicInfraDedicatedGate = $true
    $IncludeEconomicServiceSurfaceGate = $true
    $IncludeOpsControlSurfaceGate = $true
    $IncludeMarketEngineTreasuryNegativeGate = $true
    $IncludeForeignRateSourceGate = $true
    $IncludeNavValuationSourceGate = $true
    $IncludeDividendBalanceSourceGate = $true
    $profileName = "full_snapshot_ga_v1"
}

if (-not $RepoRoot) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
} else {
    $RepoRoot = (Resolve-Path $RepoRoot).Path
}
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "artifacts\migration\acceptance-gate"
}

function Require-Path {
    param([string]$Path, [string]$Name)
    if (-not (Test-Path $Path)) {
        throw "missing ${Name}: $Path"
    }
}

$functionalScript = Join-Path $RepoRoot "scripts\migration\run_functional_consistency.ps1"
$performanceGateScript = Join-Path $RepoRoot "scripts\migration\run_performance_gate_seal_single.ps1"
$chainQueryRpcGateScript = Join-Path $RepoRoot "scripts\migration\run_chain_query_rpc_gate.ps1"
$governanceRpcGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_rpc_gate.ps1"
$governanceRpcMldsaFfiGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_rpc_mldsa_ffi_gate.ps1"
$headerSyncGateScript = Join-Path $RepoRoot "scripts\migration\run_header_sync_gate.ps1"
$fastStateSyncGateScript = Join-Path $RepoRoot "scripts\migration\run_fast_state_sync_gate.ps1"
$networkDosGateScript = Join-Path $RepoRoot "scripts\migration\run_network_dos_gate.ps1"
$pacemakerFailoverGateScript = Join-Path $RepoRoot "scripts\migration\run_pacemaker_failover_gate.ps1"
$slashGovernanceGateScript = Join-Path $RepoRoot "scripts\migration\run_slash_governance_gate.ps1"
$slashPolicyExternalGateScript = Join-Path $RepoRoot "scripts\migration\run_slash_policy_external_gate.ps1"
$governanceHookGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_hook_gate.ps1"
$governanceExecutionGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_execution_gate.ps1"
$governanceParam2GateScript = Join-Path $RepoRoot "scripts\migration\run_governance_param2_gate.ps1"
$governanceParam3GateScript = Join-Path $RepoRoot "scripts\migration\run_governance_param3_gate.ps1"
$governanceMarketPolicyGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_market_policy_gate.ps1"
$governanceCouncilPolicyGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_council_policy_gate.ps1"
$governanceNegativeGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_negative_gate.ps1"
$governanceAccessPolicyGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_access_policy_gate.ps1"
$governanceTokenEconomicsGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_token_economics_gate.ps1"
$governanceTreasurySpendGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_treasury_spend_gate.ps1"
$economicInfraDedicatedGateScript = Join-Path $RepoRoot "scripts\migration\run_economic_infra_dedicated_gate.ps1"
$economicServiceSurfaceGateScript = Join-Path $RepoRoot "scripts\migration\run_economic_service_surface_gate.ps1"
$opsControlSurfaceGateScript = Join-Path $RepoRoot "scripts\migration\run_ops_control_surface_gate.ps1"
$marketEngineTreasuryNegativeGateScript = Join-Path $RepoRoot "scripts\migration\run_market_engine_treasury_negative_gate.ps1"
$foreignRateSourceGateScript = Join-Path $RepoRoot "scripts\migration\run_foreign_rate_source_gate.ps1"
$navValuationSourceGateScript = Join-Path $RepoRoot "scripts\migration\run_nav_valuation_source_gate.ps1"
$dividendBalanceSourceGateScript = Join-Path $RepoRoot "scripts\migration\run_dividend_balance_source_gate.ps1"
$unifiedAccountGateScript = Join-Path $RepoRoot "scripts\migration\run_unified_account_gate.ps1"
$rpcExposureGateScript = Join-Path $RepoRoot "scripts\migration\run_rpc_exposure_gate.ps1"
$testnetBootstrapGateScript = Join-Path $RepoRoot "scripts\migration\run_testnet_bootstrap_gate.ps1"
$unjailCooldownGateScript = Join-Path $RepoRoot "scripts\migration\run_unjail_cooldown_gate.ps1"
$adapterStabilityScript = Join-Path $RepoRoot "scripts\migration\run_adapter_stability_gate.ps1"
$vmRuntimeSplitScript = Join-Path $RepoRoot "scripts\migration\run_vm_runtime_split_gate.ps1"
$evmChainProfileSignalScript = Join-Path $RepoRoot "scripts\migration\run_evm_chain_profile_signal.ps1"
$evmTxTypeSignalScript = Join-Path $RepoRoot "scripts\migration\run_evm_tx_type_signal.ps1"
$overlapRouterSignalScript = Join-Path $RepoRoot "scripts\migration\run_overlap_router_signal.ps1"
$evmBackendCompareGateScript = Join-Path $RepoRoot "scripts\migration\run_evm_backend_compare_signal.ps1"
Require-Path -Path $functionalScript -Name "functional script"
if ($IncludePerformanceGate) {
    Require-Path -Path $performanceGateScript -Name "performance gate script"
}
if ($IncludeChainQueryRpcGate) {
    Require-Path -Path $chainQueryRpcGateScript -Name "chain query rpc gate script"
}
if ($IncludeGovernanceRpcGate) {
    Require-Path -Path $governanceRpcGateScript -Name "governance rpc gate script"
}
if ($IncludeGovernanceRpcMldsaFfiGate) {
    Require-Path -Path $governanceRpcMldsaFfiGateScript -Name "governance rpc mldsa ffi gate script"
}
if ($IncludeHeaderSyncGate) {
    Require-Path -Path $headerSyncGateScript -Name "header sync gate script"
}
if ($IncludeFastStateSyncGate) {
    Require-Path -Path $fastStateSyncGateScript -Name "fast/state sync gate script"
}
if ($IncludeNetworkDosGate) {
    Require-Path -Path $networkDosGateScript -Name "network dos gate script"
}
if ($IncludePacemakerFailoverGate) {
    Require-Path -Path $pacemakerFailoverGateScript -Name "pacemaker failover gate script"
}
if ($IncludeSlashGovernanceGate) {
    Require-Path -Path $slashGovernanceGateScript -Name "slash governance gate script"
}
if ($IncludeSlashPolicyExternalGate) {
    Require-Path -Path $slashPolicyExternalGateScript -Name "slash policy external gate script"
}
if ($IncludeGovernanceHookGate) {
    Require-Path -Path $governanceHookGateScript -Name "governance hook gate script"
}
if ($IncludeGovernanceExecutionGate) {
    Require-Path -Path $governanceExecutionGateScript -Name "governance execution gate script"
}
if ($IncludeGovernanceParam2Gate) {
    Require-Path -Path $governanceParam2GateScript -Name "governance param2 gate script"
}
if ($IncludeGovernanceParam3Gate) {
    Require-Path -Path $governanceParam3GateScript -Name "governance param3 gate script"
}
if ($IncludeGovernanceMarketPolicyGate) {
    Require-Path -Path $governanceMarketPolicyGateScript -Name "governance market policy gate script"
}
if ($IncludeGovernanceCouncilPolicyGate) {
    Require-Path -Path $governanceCouncilPolicyGateScript -Name "governance council policy gate script"
}
if ($IncludeGovernanceNegativeGate) {
    Require-Path -Path $governanceNegativeGateScript -Name "governance negative gate script"
}
if ($IncludeGovernanceAccessPolicyGate) {
    Require-Path -Path $governanceAccessPolicyGateScript -Name "governance access policy gate script"
}
if ($IncludeGovernanceTokenEconomicsGate) {
    Require-Path -Path $governanceTokenEconomicsGateScript -Name "governance token economics gate script"
}
if ($IncludeGovernanceTreasurySpendGate) {
    Require-Path -Path $governanceTreasurySpendGateScript -Name "governance treasury spend gate script"
}
if ($IncludeEconomicInfraDedicatedGate) {
    Require-Path -Path $economicInfraDedicatedGateScript -Name "economic infra dedicated gate script"
}
if ($IncludeEconomicServiceSurfaceGate) {
    Require-Path -Path $economicServiceSurfaceGateScript -Name "economic service surface gate script"
}
if ($IncludeOpsControlSurfaceGate) {
    Require-Path -Path $opsControlSurfaceGateScript -Name "ops control surface gate script"
}
if ($IncludeMarketEngineTreasuryNegativeGate) {
    Require-Path -Path $marketEngineTreasuryNegativeGateScript -Name "market engine treasury negative gate script"
}
if ($IncludeForeignRateSourceGate) {
    Require-Path -Path $foreignRateSourceGateScript -Name "foreign rate source gate script"
}
if ($IncludeNavValuationSourceGate) {
    Require-Path -Path $navValuationSourceGateScript -Name "nav valuation source gate script"
}
if ($IncludeDividendBalanceSourceGate) {
    Require-Path -Path $dividendBalanceSourceGateScript -Name "dividend balance source gate script"
}
if ($IncludeUnifiedAccountGate) {
    Require-Path -Path $unifiedAccountGateScript -Name "unified account gate script"
}
if ($IncludeRpcExposureGate) {
    Require-Path -Path $rpcExposureGateScript -Name "rpc exposure gate script"
}
if ($IncludeTestnetBootstrapGate) {
    Require-Path -Path $testnetBootstrapGateScript -Name "testnet bootstrap gate script"
}
if ($IncludeUnjailCooldownGate) {
    Require-Path -Path $unjailCooldownGateScript -Name "unjail cooldown gate script"
}
if ($IncludeAdapterStabilityGate) {
    Require-Path -Path $adapterStabilityScript -Name "adapter stability gate script"
}
if ($IncludeVmRuntimeSplitGate) {
    Require-Path -Path $vmRuntimeSplitScript -Name "vm-runtime split gate script"
}
if ($IncludeEvmChainProfileSignalGate) {
    Require-Path -Path $evmChainProfileSignalScript -Name "evm chain profile signal script"
}
if ($IncludeEvmTxTypeSignalGate) {
    Require-Path -Path $evmTxTypeSignalScript -Name "evm tx type signal script"
}
if ($IncludeOverlapRouterSignalGate) {
    Require-Path -Path $overlapRouterSignalScript -Name "overlap router signal script"
}
if ($IncludeEvmBackendCompareGate) {
    Require-Path -Path $evmBackendCompareGateScript -Name "evm backend compare gate script"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$functionalOutputDir = Join-Path $OutputDir "functional"
$performanceOutputDir = Join-Path $OutputDir "performance-gate"
$chainQueryRpcOutputDir = Join-Path $OutputDir "chain-query-rpc-gate"
$governanceRpcOutputDir = Join-Path $OutputDir "governance-rpc-gate"
$governanceRpcMldsaFfiOutputDir = Join-Path $OutputDir "governance-rpc-mldsa-ffi-gate"
$headerSyncOutputDir = Join-Path $OutputDir "header-sync-gate"
$fastStateSyncOutputDir = Join-Path $OutputDir "fast-state-sync-gate"
$networkDosOutputDir = Join-Path $OutputDir "network-dos-gate"
$pacemakerFailoverOutputDir = Join-Path $OutputDir "pacemaker-failover-gate"
$slashGovernanceOutputDir = Join-Path $OutputDir "slash-governance-gate"
$slashPolicyExternalOutputDir = Join-Path $OutputDir "slash-policy-external-gate"
$governanceHookOutputDir = Join-Path $OutputDir "governance-hook-gate"
$governanceExecutionOutputDir = Join-Path $OutputDir "governance-execution-gate"
$governanceParam2OutputDir = Join-Path $OutputDir "governance-param2-gate"
$governanceParam3OutputDir = Join-Path $OutputDir "governance-param3-gate"
$governanceMarketPolicyOutputDir = Join-Path $OutputDir "governance-market-policy-gate"
$governanceCouncilPolicyOutputDir = Join-Path $OutputDir "governance-council-policy-gate"
$governanceNegativeOutputDir = Join-Path $OutputDir "governance-negative-gate"
$governanceAccessPolicyOutputDir = Join-Path $OutputDir "governance-access-policy-gate"
$governanceTokenEconomicsOutputDir = Join-Path $OutputDir "governance-token-economics-gate"
$governanceTreasurySpendOutputDir = Join-Path $OutputDir "governance-treasury-spend-gate"
$economicInfraDedicatedOutputDir = Join-Path $OutputDir "economic-infra-dedicated-gate"
$economicServiceSurfaceOutputDir = Join-Path $OutputDir "economic-service-surface-gate"
$opsControlSurfaceOutputDir = Join-Path $OutputDir "ops-control-surface-gate"
$marketEngineTreasuryNegativeOutputDir = Join-Path $OutputDir "market-engine-treasury-negative-gate"
$foreignRateSourceOutputDir = Join-Path $OutputDir "foreign-rate-source-gate"
$navValuationSourceOutputDir = Join-Path $OutputDir "nav-valuation-source-gate"
$dividendBalanceSourceOutputDir = Join-Path $OutputDir "dividend-balance-source-gate"
$unifiedAccountOutputDir = Join-Path $OutputDir "unified-account-gate"
$rpcExposureOutputDir = Join-Path $OutputDir "rpc-exposure-gate"
$testnetBootstrapOutputDir = Join-Path $OutputDir "testnet-bootstrap-gate"
$unjailCooldownOutputDir = Join-Path $OutputDir "unjail-cooldown-gate"
$adapterStabilityOutputDir = Join-Path $OutputDir "adapter-stability-gate"
$vmRuntimeSplitOutputDir = Join-Path $OutputDir "vm-runtime-split-gate"
$evmChainProfileSignalOutputDir = Join-Path $OutputDir "evm-chain-profile-signal-gate"
$evmTxTypeSignalOutputDir = Join-Path $OutputDir "evm-tx-type-signal-gate"
$overlapRouterSignalOutputDir = Join-Path $OutputDir "overlap-router-signal-gate"
$evmBackendCompareOutputDir = Join-Path $OutputDir "evm-backend-compare-gate"
$evmBackendCompareEvmOutputDir = Join-Path $evmBackendCompareOutputDir "evm"
$evmBackendComparePolygonOutputDir = Join-Path $evmBackendCompareOutputDir "polygon"
$evmBackendCompareBnbOutputDir = Join-Path $evmBackendCompareOutputDir "bnb"
$evmBackendCompareAvalancheOutputDir = Join-Path $evmBackendCompareOutputDir "avalanche"
New-Item -ItemType Directory -Force -Path $functionalOutputDir | Out-Null
New-Item -ItemType Directory -Force -Path $performanceOutputDir | Out-Null
if ($IncludeChainQueryRpcGate) {
    New-Item -ItemType Directory -Force -Path $chainQueryRpcOutputDir | Out-Null
}
if ($IncludeGovernanceRpcGate) {
    New-Item -ItemType Directory -Force -Path $governanceRpcOutputDir | Out-Null
}
if ($IncludeGovernanceRpcMldsaFfiGate) {
    New-Item -ItemType Directory -Force -Path $governanceRpcMldsaFfiOutputDir | Out-Null
}
if ($IncludeHeaderSyncGate) {
    New-Item -ItemType Directory -Force -Path $headerSyncOutputDir | Out-Null
}
if ($IncludeFastStateSyncGate) {
    New-Item -ItemType Directory -Force -Path $fastStateSyncOutputDir | Out-Null
}
if ($IncludeNetworkDosGate) {
    New-Item -ItemType Directory -Force -Path $networkDosOutputDir | Out-Null
}
if ($IncludePacemakerFailoverGate) {
    New-Item -ItemType Directory -Force -Path $pacemakerFailoverOutputDir | Out-Null
}
if ($IncludeSlashGovernanceGate) {
    New-Item -ItemType Directory -Force -Path $slashGovernanceOutputDir | Out-Null
}
if ($IncludeSlashPolicyExternalGate) {
    New-Item -ItemType Directory -Force -Path $slashPolicyExternalOutputDir | Out-Null
}
if ($IncludeGovernanceHookGate) {
    New-Item -ItemType Directory -Force -Path $governanceHookOutputDir | Out-Null
}
if ($IncludeGovernanceExecutionGate) {
    New-Item -ItemType Directory -Force -Path $governanceExecutionOutputDir | Out-Null
}
if ($IncludeGovernanceParam2Gate) {
    New-Item -ItemType Directory -Force -Path $governanceParam2OutputDir | Out-Null
}
if ($IncludeGovernanceParam3Gate) {
    New-Item -ItemType Directory -Force -Path $governanceParam3OutputDir | Out-Null
}
if ($IncludeGovernanceMarketPolicyGate) {
    New-Item -ItemType Directory -Force -Path $governanceMarketPolicyOutputDir | Out-Null
}
if ($IncludeGovernanceCouncilPolicyGate) {
    New-Item -ItemType Directory -Force -Path $governanceCouncilPolicyOutputDir | Out-Null
}
if ($IncludeGovernanceNegativeGate) {
    New-Item -ItemType Directory -Force -Path $governanceNegativeOutputDir | Out-Null
}
if ($IncludeGovernanceAccessPolicyGate) {
    New-Item -ItemType Directory -Force -Path $governanceAccessPolicyOutputDir | Out-Null
}
if ($IncludeGovernanceTokenEconomicsGate) {
    New-Item -ItemType Directory -Force -Path $governanceTokenEconomicsOutputDir | Out-Null
}
if ($IncludeGovernanceTreasurySpendGate) {
    New-Item -ItemType Directory -Force -Path $governanceTreasurySpendOutputDir | Out-Null
}
if ($IncludeEconomicInfraDedicatedGate) {
    New-Item -ItemType Directory -Force -Path $economicInfraDedicatedOutputDir | Out-Null
}
if ($IncludeEconomicServiceSurfaceGate) {
    New-Item -ItemType Directory -Force -Path $economicServiceSurfaceOutputDir | Out-Null
}
if ($IncludeOpsControlSurfaceGate) {
    New-Item -ItemType Directory -Force -Path $opsControlSurfaceOutputDir | Out-Null
}
if ($IncludeMarketEngineTreasuryNegativeGate) {
    New-Item -ItemType Directory -Force -Path $marketEngineTreasuryNegativeOutputDir | Out-Null
}
if ($IncludeForeignRateSourceGate) {
    New-Item -ItemType Directory -Force -Path $foreignRateSourceOutputDir | Out-Null
}
if ($IncludeNavValuationSourceGate) {
    New-Item -ItemType Directory -Force -Path $navValuationSourceOutputDir | Out-Null
}
if ($IncludeDividendBalanceSourceGate) {
    New-Item -ItemType Directory -Force -Path $dividendBalanceSourceOutputDir | Out-Null
}
if ($IncludeUnifiedAccountGate) {
    New-Item -ItemType Directory -Force -Path $unifiedAccountOutputDir | Out-Null
}
if ($IncludeRpcExposureGate) {
    New-Item -ItemType Directory -Force -Path $rpcExposureOutputDir | Out-Null
}
if ($IncludeTestnetBootstrapGate) {
    New-Item -ItemType Directory -Force -Path $testnetBootstrapOutputDir | Out-Null
}
if ($IncludeUnjailCooldownGate) {
    New-Item -ItemType Directory -Force -Path $unjailCooldownOutputDir | Out-Null
}
if ($IncludeAdapterStabilityGate) {
    New-Item -ItemType Directory -Force -Path $adapterStabilityOutputDir | Out-Null
}
if ($IncludeVmRuntimeSplitGate) {
    New-Item -ItemType Directory -Force -Path $vmRuntimeSplitOutputDir | Out-Null
}
if ($IncludeEvmChainProfileSignalGate) {
    New-Item -ItemType Directory -Force -Path $evmChainProfileSignalOutputDir | Out-Null
}
if ($IncludeEvmTxTypeSignalGate) {
    New-Item -ItemType Directory -Force -Path $evmTxTypeSignalOutputDir | Out-Null
}
if ($IncludeOverlapRouterSignalGate) {
    New-Item -ItemType Directory -Force -Path $overlapRouterSignalOutputDir | Out-Null
}
if ($IncludeEvmBackendCompareGate) {
    New-Item -ItemType Directory -Force -Path $evmBackendCompareEvmOutputDir | Out-Null
    if ($EvmBackendCompareIncludePolygon) {
        New-Item -ItemType Directory -Force -Path $evmBackendComparePolygonOutputDir | Out-Null
    }
    if ($EvmBackendCompareIncludeBnb) {
        New-Item -ItemType Directory -Force -Path $evmBackendCompareBnbOutputDir | Out-Null
    }
    if ($EvmBackendCompareIncludeAvalanche) {
        New-Item -ItemType Directory -Force -Path $evmBackendCompareAvalancheOutputDir | Out-Null
    }
}

Write-Host "acceptance gate: functional consistency ..."
& $functionalScript `
    -RepoRoot $RepoRoot `
    -OutputDir $functionalOutputDir `
    -CapabilityVariant core | Out-Null

if ($IncludePerformanceGate) {
    Write-Host "acceptance gate: performance seal gate ..."
    $performanceAttempt = 0
    while ($true) {
        try {
            & $performanceGateScript `
                -RepoRoot $RepoRoot `
                -OutputDir $performanceOutputDir `
                -AllowedRegressionPct $AllowedRegressionPct `
                -Runs $PerformanceRuns | Out-Null
            break
        } catch {
            $canRetry = $false
            if ($performanceAttempt -lt $PerformanceBorderlineRetries) {
                $perfSummaryPath = Join-Path $performanceOutputDir "performance-gate-summary.json"
                if (Test-Path $perfSummaryPath) {
                    try {
                        $perfSummary = Get-Content -Path $perfSummaryPath -Raw | ConvertFrom-Json
                        $failedRows = @($perfSummary.compare | Where-Object { -not [bool]$_.pass })
                        if ($failedRows.Count -gt 0) {
                            $borderlineThreshold = $AllowedRegressionPct - $PerformanceBorderlineEpsilonPct
                            $hardFailures = @($failedRows | Where-Object { [double]$_.delta_pct -lt $borderlineThreshold })
                            $canRetry = ($hardFailures.Count -eq 0)
                        }
                    } catch {
                        $canRetry = $false
                    }
                }
            }
            if ($canRetry) {
                $performanceAttempt++
                Write-Host "acceptance gate: performance seal gate borderline retry ($performanceAttempt/$PerformanceBorderlineRetries) ..."
                continue
            }
            throw
        }
    }
} else {
    Write-Host "acceptance gate: performance seal gate skipped (IncludePerformanceGate=false)"
}

if ($IncludeChainQueryRpcGate) {
    Write-Host "acceptance gate: chain query rpc gate ..."
    & $chainQueryRpcGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $chainQueryRpcOutputDir `
        -Bind $ChainQueryRpcBind `
        -ExpectedRequests $ChainQueryRpcExpectedRequests | Out-Null
}

if ($IncludeGovernanceRpcGate) {
    Write-Host "acceptance gate: governance rpc gate ..."
    & $governanceRpcGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceRpcOutputDir `
        -Bind $GovernanceRpcBind `
        -StartupTimeoutSeconds 20 `
        -ExpectedRequests $GovernanceRpcExpectedRequests | Out-Null
}

if ($IncludeGovernanceRpcMldsaFfiGate) {
    Write-Host "acceptance gate: governance rpc mldsa ffi gate ..."
    if (-not [string]::IsNullOrWhiteSpace($GovernanceRpcMldsaFfiAoemRoot)) {
        & $governanceRpcMldsaFfiGateScript `
            -RepoRoot $RepoRoot `
            -AoemRoot $GovernanceRpcMldsaFfiAoemRoot `
            -OutputDir $governanceRpcMldsaFfiOutputDir `
            -Bind $GovernanceRpcMldsaFfiBind `
            -ExpectedRequests $GovernanceRpcMldsaFfiExpectedRequests | Out-Null
    } else {
        & $governanceRpcMldsaFfiGateScript `
            -RepoRoot $RepoRoot `
            -OutputDir $governanceRpcMldsaFfiOutputDir `
            -Bind $GovernanceRpcMldsaFfiBind `
            -ExpectedRequests $GovernanceRpcMldsaFfiExpectedRequests | Out-Null
    }
}

if ($IncludeHeaderSyncGate) {
    Write-Host "acceptance gate: header sync gate ..."
    & $headerSyncGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $headerSyncOutputDir | Out-Null
}

if ($IncludeFastStateSyncGate) {
    Write-Host "acceptance gate: fast/state sync gate ..."
    & $fastStateSyncGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $fastStateSyncOutputDir | Out-Null
}

if ($IncludeNetworkDosGate) {
    Write-Host "acceptance gate: network dos gate ..."
    & $networkDosGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $networkDosOutputDir | Out-Null
}

if ($IncludePacemakerFailoverGate) {
    Write-Host "acceptance gate: pacemaker failover gate ..."
    & $pacemakerFailoverGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $pacemakerFailoverOutputDir `
        -Nodes $PacemakerFailoverNodes `
        -FailedLeader $PacemakerFailoverFailedLeader | Out-Null
}

if ($IncludeSlashGovernanceGate) {
    Write-Host "acceptance gate: slash governance gate ..."
    & $slashGovernanceGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $slashGovernanceOutputDir | Out-Null
}

if ($IncludeSlashPolicyExternalGate) {
    Write-Host "acceptance gate: slash policy external gate ..."
    & $slashPolicyExternalGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $slashPolicyExternalOutputDir | Out-Null
}

if ($IncludeGovernanceHookGate) {
    Write-Host "acceptance gate: governance hook gate ..."
    & $governanceHookGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceHookOutputDir | Out-Null
}

if ($IncludeGovernanceExecutionGate) {
    Write-Host "acceptance gate: governance execution gate ..."
    & $governanceExecutionGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceExecutionOutputDir | Out-Null
}

if ($IncludeGovernanceParam2Gate) {
    Write-Host "acceptance gate: governance param2 gate ..."
    & $governanceParam2GateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceParam2OutputDir | Out-Null
}

if ($IncludeGovernanceParam3Gate) {
    Write-Host "acceptance gate: governance param3 gate ..."
    & $governanceParam3GateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceParam3OutputDir | Out-Null
}

if ($IncludeGovernanceMarketPolicyGate) {
    Write-Host "acceptance gate: governance market policy gate ..."
    & $governanceMarketPolicyGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceMarketPolicyOutputDir | Out-Null
}

if ($IncludeGovernanceCouncilPolicyGate) {
    Write-Host "acceptance gate: governance council policy gate ..."
    & $governanceCouncilPolicyGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceCouncilPolicyOutputDir | Out-Null
}

if ($IncludeGovernanceNegativeGate) {
    Write-Host "acceptance gate: governance negative gate ..."
    & $governanceNegativeGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceNegativeOutputDir | Out-Null
}

if ($IncludeGovernanceAccessPolicyGate) {
    Write-Host "acceptance gate: governance access policy gate ..."
    & $governanceAccessPolicyGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceAccessPolicyOutputDir | Out-Null
}

if ($IncludeGovernanceTokenEconomicsGate) {
    Write-Host "acceptance gate: governance token economics gate ..."
    & $governanceTokenEconomicsGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceTokenEconomicsOutputDir | Out-Null
}

if ($IncludeGovernanceTreasurySpendGate) {
    Write-Host "acceptance gate: governance treasury spend gate ..."
    & $governanceTreasurySpendGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceTreasurySpendOutputDir | Out-Null
}

if ($IncludeEconomicInfraDedicatedGate) {
    Write-Host "acceptance gate: economic infra dedicated gate ..."
    $useExistingEconomicInfraSubGates = [bool](
        $IncludeGovernanceMarketPolicyGate -and
        $IncludeGovernanceTokenEconomicsGate -and
        $IncludeGovernanceTreasurySpendGate
    )
    if ($useExistingEconomicInfraSubGates) {
        $governanceMarketPolicySummaryForEconomicInfra = Join-Path $governanceMarketPolicyOutputDir "governance-market-policy-gate-summary.json"
        $governanceTokenEconomicsSummaryForEconomicInfra = Join-Path $governanceTokenEconomicsOutputDir "governance-token-economics-gate-summary.json"
        $governanceTreasurySpendSummaryForEconomicInfra = Join-Path $governanceTreasurySpendOutputDir "governance-treasury-spend-gate-summary.json"
        & $economicInfraDedicatedGateScript `
            -RepoRoot $RepoRoot `
            -OutputDir $economicInfraDedicatedOutputDir `
            -RunSubGates $false `
            -GovernanceMarketPolicySummaryJson $governanceMarketPolicySummaryForEconomicInfra `
            -GovernanceTokenEconomicsSummaryJson $governanceTokenEconomicsSummaryForEconomicInfra `
            -GovernanceTreasurySpendSummaryJson $governanceTreasurySpendSummaryForEconomicInfra | Out-Null
    } else {
        & $economicInfraDedicatedGateScript `
            -RepoRoot $RepoRoot `
            -OutputDir $economicInfraDedicatedOutputDir | Out-Null
    }
}

if ($IncludeEconomicServiceSurfaceGate) {
    Write-Host "acceptance gate: economic service surface gate ..."
    & $economicServiceSurfaceGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $economicServiceSurfaceOutputDir | Out-Null
}

if ($IncludeOpsControlSurfaceGate) {
    Write-Host "acceptance gate: ops control surface gate ..."
    & $opsControlSurfaceGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $opsControlSurfaceOutputDir | Out-Null
}

if ($IncludeMarketEngineTreasuryNegativeGate) {
    Write-Host "acceptance gate: market engine treasury negative gate ..."
    & $marketEngineTreasuryNegativeGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $marketEngineTreasuryNegativeOutputDir | Out-Null
}

if ($IncludeForeignRateSourceGate) {
    Write-Host "acceptance gate: foreign rate source gate ..."
    & $foreignRateSourceGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $foreignRateSourceOutputDir | Out-Null
}

if ($IncludeNavValuationSourceGate) {
    Write-Host "acceptance gate: nav valuation source gate ..."
    & $navValuationSourceGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $navValuationSourceOutputDir | Out-Null
}

if ($IncludeDividendBalanceSourceGate) {
    Write-Host "acceptance gate: dividend balance source gate ..."
    & $dividendBalanceSourceGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $dividendBalanceSourceOutputDir | Out-Null
}

if ($IncludeUnifiedAccountGate) {
    Write-Host "acceptance gate: unified account gate ..."
    & $unifiedAccountGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $unifiedAccountOutputDir | Out-Null
}

if ($IncludeRpcExposureGate) {
    Write-Host "acceptance gate: rpc exposure gate ..."
    & $rpcExposureGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $rpcExposureOutputDir `
        -PublicBind $RpcExposurePublicBind `
        -GovBind $RpcExposureGovBind | Out-Null
}
if ($IncludeTestnetBootstrapGate) {
    Write-Host "acceptance gate: testnet bootstrap gate ..."
    & $testnetBootstrapGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $testnetBootstrapOutputDir | Out-Null
}

if ($IncludeUnjailCooldownGate) {
    Write-Host "acceptance gate: unjail cooldown gate ..."
    & $unjailCooldownGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $unjailCooldownOutputDir | Out-Null
}

if ($IncludeAdapterStabilityGate) {
    Write-Host "acceptance gate: adapter stability gate ..."
    & $adapterStabilityScript `
        -RepoRoot $RepoRoot `
        -OutputDir $adapterStabilityOutputDir `
        -Runs $AdapterStabilityRuns | Out-Null
}
if ($IncludeVmRuntimeSplitGate) {
    Write-Host "acceptance gate: vm-runtime split gate ..."
    & $vmRuntimeSplitScript `
        -RepoRoot $RepoRoot `
        -OutputDir $vmRuntimeSplitOutputDir | Out-Null
}
if ($IncludeEvmChainProfileSignalGate) {
    Write-Host "acceptance gate: evm chain profile signal gate ..."
    & $evmChainProfileSignalScript `
        -RepoRoot $RepoRoot `
        -OutputDir $evmChainProfileSignalOutputDir | Out-Null
}
if ($IncludeEvmTxTypeSignalGate) {
    Write-Host "acceptance gate: evm tx type signal gate ..."
    & $evmTxTypeSignalScript `
        -RepoRoot $RepoRoot `
        -OutputDir $evmTxTypeSignalOutputDir | Out-Null
}
if ($IncludeOverlapRouterSignalGate) {
    Write-Host "acceptance gate: overlap router signal gate ..."
    & $overlapRouterSignalScript `
        -RepoRoot $RepoRoot `
        -OutputDir $overlapRouterSignalOutputDir | Out-Null
}
if ($IncludeEvmBackendCompareGate) {
    Write-Host "acceptance gate: evm backend compare gate (evm) ..."
    & $evmBackendCompareGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $evmBackendCompareEvmOutputDir `
        -AdapterChain evm | Out-Null
    if ($EvmBackendCompareIncludePolygon) {
        Write-Host "acceptance gate: evm backend compare gate (polygon) ..."
        & $evmBackendCompareGateScript `
            -RepoRoot $RepoRoot `
            -OutputDir $evmBackendComparePolygonOutputDir `
            -AdapterChain polygon | Out-Null
    }
    if ($EvmBackendCompareIncludeBnb) {
        Write-Host "acceptance gate: evm backend compare gate (bnb) ..."
        & $evmBackendCompareGateScript `
            -RepoRoot $RepoRoot `
            -OutputDir $evmBackendCompareBnbOutputDir `
            -AdapterChain bnb | Out-Null
    }
    if ($EvmBackendCompareIncludeAvalanche) {
        Write-Host "acceptance gate: evm backend compare gate (avalanche) ..."
        & $evmBackendCompareGateScript `
            -RepoRoot $RepoRoot `
            -OutputDir $evmBackendCompareAvalancheOutputDir `
            -AdapterChain avalanche | Out-Null
    }
}

$functionalJson = Join-Path $functionalOutputDir "functional-consistency.json"
$performanceJson = Join-Path $performanceOutputDir "performance-gate-summary.json"
if ($IncludeChainQueryRpcGate) {
    $chainQueryRpcJson = Join-Path $chainQueryRpcOutputDir "chain-query-rpc-gate-summary.json"
}
if ($IncludeGovernanceRpcGate) {
    $governanceRpcJson = Join-Path $governanceRpcOutputDir "governance-rpc-gate-summary.json"
}
if ($IncludeGovernanceRpcMldsaFfiGate) {
    $governanceRpcMldsaFfiJson = Join-Path $governanceRpcMldsaFfiOutputDir "governance-rpc-mldsa-ffi-gate-summary.json"
}
if ($IncludeHeaderSyncGate) {
    $headerSyncJson = Join-Path $headerSyncOutputDir "header-sync-gate-summary.json"
}
if ($IncludeFastStateSyncGate) {
    $fastStateSyncJson = Join-Path $fastStateSyncOutputDir "fast-state-sync-gate-summary.json"
}
if ($IncludeNetworkDosGate) {
    $networkDosJson = Join-Path $networkDosOutputDir "network-dos-gate-summary.json"
}
if ($IncludePacemakerFailoverGate) {
    $pacemakerFailoverJson = Join-Path $pacemakerFailoverOutputDir "pacemaker-failover-gate-summary.json"
}
if ($IncludeSlashGovernanceGate) {
    $slashGovernanceJson = Join-Path $slashGovernanceOutputDir "slash-governance-gate-summary.json"
}
if ($IncludeSlashPolicyExternalGate) {
    $slashPolicyExternalJson = Join-Path $slashPolicyExternalOutputDir "slash-policy-external-gate-summary.json"
}
if ($IncludeGovernanceHookGate) {
    $governanceHookJson = Join-Path $governanceHookOutputDir "governance-hook-gate-summary.json"
}
if ($IncludeGovernanceExecutionGate) {
    $governanceExecutionJson = Join-Path $governanceExecutionOutputDir "governance-execution-gate-summary.json"
}
if ($IncludeGovernanceParam2Gate) {
    $governanceParam2Json = Join-Path $governanceParam2OutputDir "governance-param2-gate-summary.json"
}
if ($IncludeGovernanceParam3Gate) {
    $governanceParam3Json = Join-Path $governanceParam3OutputDir "governance-param3-gate-summary.json"
}
if ($IncludeGovernanceMarketPolicyGate) {
    $governanceMarketPolicyJson = Join-Path $governanceMarketPolicyOutputDir "governance-market-policy-gate-summary.json"
}
if ($IncludeGovernanceCouncilPolicyGate) {
    $governanceCouncilPolicyJson = Join-Path $governanceCouncilPolicyOutputDir "governance-council-policy-gate-summary.json"
}
if ($IncludeGovernanceNegativeGate) {
    $governanceNegativeJson = Join-Path $governanceNegativeOutputDir "governance-negative-gate-summary.json"
}
if ($IncludeGovernanceAccessPolicyGate) {
    $governanceAccessPolicyJson = Join-Path $governanceAccessPolicyOutputDir "governance-access-policy-gate-summary.json"
}
if ($IncludeGovernanceTokenEconomicsGate) {
    $governanceTokenEconomicsJson = Join-Path $governanceTokenEconomicsOutputDir "governance-token-economics-gate-summary.json"
}
if ($IncludeGovernanceTreasurySpendGate) {
    $governanceTreasurySpendJson = Join-Path $governanceTreasurySpendOutputDir "governance-treasury-spend-gate-summary.json"
}
if ($IncludeEconomicInfraDedicatedGate) {
    $economicInfraDedicatedJson = Join-Path $economicInfraDedicatedOutputDir "economic-infra-dedicated-gate-summary.json"
}
if ($IncludeEconomicServiceSurfaceGate) {
    $economicServiceSurfaceJson = Join-Path $economicServiceSurfaceOutputDir "economic-service-surface-gate-summary.json"
}
if ($IncludeOpsControlSurfaceGate) {
    $opsControlSurfaceJson = Join-Path $opsControlSurfaceOutputDir "ops-control-surface-gate-summary.json"
}
if ($IncludeMarketEngineTreasuryNegativeGate) {
    $marketEngineTreasuryNegativeJson = Join-Path $marketEngineTreasuryNegativeOutputDir "market-engine-treasury-negative-gate-summary.json"
}
if ($IncludeForeignRateSourceGate) {
    $foreignRateSourceJson = Join-Path $foreignRateSourceOutputDir "foreign-rate-source-gate-summary.json"
}
if ($IncludeNavValuationSourceGate) {
    $navValuationSourceJson = Join-Path $navValuationSourceOutputDir "nav-valuation-source-gate-summary.json"
}
if ($IncludeDividendBalanceSourceGate) {
    $dividendBalanceSourceJson = Join-Path $dividendBalanceSourceOutputDir "dividend-balance-source-gate-summary.json"
}
if ($IncludeUnifiedAccountGate) {
    $unifiedAccountJson = Join-Path $unifiedAccountOutputDir "unified-account-gate-summary.json"
}
if ($IncludeRpcExposureGate) {
    $rpcExposureJson = Join-Path $rpcExposureOutputDir "rpc-exposure-gate-summary.json"
}
if ($IncludeTestnetBootstrapGate) {
    $testnetBootstrapJson = Join-Path $testnetBootstrapOutputDir "testnet-bootstrap-gate-summary.json"
}
if ($IncludeUnjailCooldownGate) {
    $unjailCooldownJson = Join-Path $unjailCooldownOutputDir "unjail-cooldown-gate-summary.json"
}
if ($IncludeAdapterStabilityGate) {
    $adapterStabilityJson = Join-Path $adapterStabilityOutputDir "adapter-stability-summary.json"
}
if ($IncludeVmRuntimeSplitGate) {
    $vmRuntimeSplitJson = Join-Path $vmRuntimeSplitOutputDir "vm-runtime-split-gate-summary.json"
}
if ($IncludeEvmChainProfileSignalGate) {
    $evmChainProfileSignalJson = Join-Path $evmChainProfileSignalOutputDir "evm_chain_profile_signal.json"
}
if ($IncludeEvmTxTypeSignalGate) {
    $evmTxTypeSignalJson = Join-Path $evmTxTypeSignalOutputDir "tx_type_compat_signal.json"
}
if ($IncludeOverlapRouterSignalGate) {
    $overlapRouterSignalJson = Join-Path $overlapRouterSignalOutputDir "overlap_router_signal.json"
}
if ($IncludeEvmBackendCompareGate) {
    $evmBackendCompareEvmJson = Join-Path $evmBackendCompareEvmOutputDir "backend_compare_signal.json"
    if ($EvmBackendCompareIncludePolygon) {
        $evmBackendComparePolygonJson = Join-Path $evmBackendComparePolygonOutputDir "backend_compare_signal.json"
    }
    if ($EvmBackendCompareIncludeBnb) {
        $evmBackendCompareBnbJson = Join-Path $evmBackendCompareBnbOutputDir "backend_compare_signal.json"
    }
    if ($EvmBackendCompareIncludeAvalanche) {
        $evmBackendCompareAvalancheJson = Join-Path $evmBackendCompareAvalancheOutputDir "backend_compare_signal.json"
    }
}
Require-Path -Path $functionalJson -Name "functional report json"
if ($IncludePerformanceGate) {
    Require-Path -Path $performanceJson -Name "performance gate summary json"
}
if ($IncludeChainQueryRpcGate) {
    Require-Path -Path $chainQueryRpcJson -Name "chain query rpc gate summary json"
}
if ($IncludeGovernanceRpcGate) {
    Require-Path -Path $governanceRpcJson -Name "governance rpc gate summary json"
}
if ($IncludeGovernanceRpcMldsaFfiGate) {
    Require-Path -Path $governanceRpcMldsaFfiJson -Name "governance rpc mldsa ffi gate summary json"
}
if ($IncludeHeaderSyncGate) {
    Require-Path -Path $headerSyncJson -Name "header sync gate summary json"
}
if ($IncludeFastStateSyncGate) {
    Require-Path -Path $fastStateSyncJson -Name "fast/state sync gate summary json"
}
if ($IncludeNetworkDosGate) {
    Require-Path -Path $networkDosJson -Name "network dos gate summary json"
}
if ($IncludePacemakerFailoverGate) {
    Require-Path -Path $pacemakerFailoverJson -Name "pacemaker failover gate summary json"
}
if ($IncludeSlashGovernanceGate) {
    Require-Path -Path $slashGovernanceJson -Name "slash governance gate summary json"
}
if ($IncludeSlashPolicyExternalGate) {
    Require-Path -Path $slashPolicyExternalJson -Name "slash policy external gate summary json"
}
if ($IncludeGovernanceHookGate) {
    Require-Path -Path $governanceHookJson -Name "governance hook gate summary json"
}
if ($IncludeGovernanceExecutionGate) {
    Require-Path -Path $governanceExecutionJson -Name "governance execution gate summary json"
}
if ($IncludeGovernanceParam2Gate) {
    Require-Path -Path $governanceParam2Json -Name "governance param2 gate summary json"
}
if ($IncludeGovernanceParam3Gate) {
    Require-Path -Path $governanceParam3Json -Name "governance param3 gate summary json"
}
if ($IncludeGovernanceMarketPolicyGate) {
    Require-Path -Path $governanceMarketPolicyJson -Name "governance market policy gate summary json"
}
if ($IncludeGovernanceCouncilPolicyGate) {
    Require-Path -Path $governanceCouncilPolicyJson -Name "governance council policy gate summary json"
}
if ($IncludeGovernanceNegativeGate) {
    Require-Path -Path $governanceNegativeJson -Name "governance negative gate summary json"
}
if ($IncludeGovernanceAccessPolicyGate) {
    Require-Path -Path $governanceAccessPolicyJson -Name "governance access policy gate summary json"
}
if ($IncludeGovernanceTokenEconomicsGate) {
    Require-Path -Path $governanceTokenEconomicsJson -Name "governance token economics gate summary json"
}
if ($IncludeGovernanceTreasurySpendGate) {
    Require-Path -Path $governanceTreasurySpendJson -Name "governance treasury spend gate summary json"
}
if ($IncludeEconomicInfraDedicatedGate) {
    Require-Path -Path $economicInfraDedicatedJson -Name "economic infra dedicated gate summary json"
}
if ($IncludeEconomicServiceSurfaceGate) {
    Require-Path -Path $economicServiceSurfaceJson -Name "economic service surface gate summary json"
}
if ($IncludeOpsControlSurfaceGate) {
    Require-Path -Path $opsControlSurfaceJson -Name "ops control surface gate summary json"
}
if ($IncludeMarketEngineTreasuryNegativeGate) {
    Require-Path -Path $marketEngineTreasuryNegativeJson -Name "market engine treasury negative gate summary json"
}
if ($IncludeForeignRateSourceGate) {
    Require-Path -Path $foreignRateSourceJson -Name "foreign rate source gate summary json"
}
if ($IncludeNavValuationSourceGate) {
    Require-Path -Path $navValuationSourceJson -Name "nav valuation source gate summary json"
}
if ($IncludeDividendBalanceSourceGate) {
    Require-Path -Path $dividendBalanceSourceJson -Name "dividend balance source gate summary json"
}
if ($IncludeUnifiedAccountGate) {
    Require-Path -Path $unifiedAccountJson -Name "unified account gate summary json"
}
if ($IncludeRpcExposureGate) {
    Require-Path -Path $rpcExposureJson -Name "rpc exposure gate summary json"
}
if ($IncludeTestnetBootstrapGate) {
    Require-Path -Path $testnetBootstrapJson -Name "testnet bootstrap gate summary json"
}
if ($IncludeUnjailCooldownGate) {
    Require-Path -Path $unjailCooldownJson -Name "unjail cooldown gate summary json"
}
if ($IncludeAdapterStabilityGate) {
    Require-Path -Path $adapterStabilityJson -Name "adapter stability summary json"
}
if ($IncludeVmRuntimeSplitGate) {
    Require-Path -Path $vmRuntimeSplitJson -Name "vm-runtime split gate summary json"
}
if ($IncludeEvmChainProfileSignalGate) {
    Require-Path -Path $evmChainProfileSignalJson -Name "evm chain profile signal json"
}
if ($IncludeEvmTxTypeSignalGate) {
    Require-Path -Path $evmTxTypeSignalJson -Name "evm tx type signal json"
}
if ($IncludeOverlapRouterSignalGate) {
    Require-Path -Path $overlapRouterSignalJson -Name "overlap router signal json"
}
if ($IncludeEvmBackendCompareGate) {
    Require-Path -Path $evmBackendCompareEvmJson -Name "evm backend compare evm signal json"
    if ($EvmBackendCompareIncludePolygon) {
        Require-Path -Path $evmBackendComparePolygonJson -Name "evm backend compare polygon signal json"
    }
    if ($EvmBackendCompareIncludeBnb) {
        Require-Path -Path $evmBackendCompareBnbJson -Name "evm backend compare bnb signal json"
    }
    if ($EvmBackendCompareIncludeAvalanche) {
        Require-Path -Path $evmBackendCompareAvalancheJson -Name "evm backend compare avalanche signal json"
    }
}

$functional = Get-Content -Path $functionalJson -Raw | ConvertFrom-Json
if ($IncludePerformanceGate) {
    $performance = Get-Content -Path $performanceJson -Raw | ConvertFrom-Json
} else {
    $performance = [pscustomobject]@{
        pass = $true
        skipped = $true
    }
}
if ($IncludeChainQueryRpcGate) {
    $chainQueryRpc = Get-Content -Path $chainQueryRpcJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceRpcGate) {
    $governanceRpc = Get-Content -Path $governanceRpcJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceRpcMldsaFfiGate) {
    $governanceRpcMldsaFfi = Get-Content -Path $governanceRpcMldsaFfiJson -Raw | ConvertFrom-Json
}
if ($IncludeHeaderSyncGate) {
    $headerSync = Get-Content -Path $headerSyncJson -Raw | ConvertFrom-Json
}
if ($IncludeFastStateSyncGate) {
    $fastStateSync = Get-Content -Path $fastStateSyncJson -Raw | ConvertFrom-Json
}
if ($IncludeNetworkDosGate) {
    $networkDos = Get-Content -Path $networkDosJson -Raw | ConvertFrom-Json
}
if ($IncludePacemakerFailoverGate) {
    $pacemakerFailover = Get-Content -Path $pacemakerFailoverJson -Raw | ConvertFrom-Json
}
if ($IncludeSlashGovernanceGate) {
    $slashGovernance = Get-Content -Path $slashGovernanceJson -Raw | ConvertFrom-Json
}
if ($IncludeSlashPolicyExternalGate) {
    $slashPolicyExternal = Get-Content -Path $slashPolicyExternalJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceHookGate) {
    $governanceHook = Get-Content -Path $governanceHookJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceExecutionGate) {
    $governanceExecution = Get-Content -Path $governanceExecutionJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceParam2Gate) {
    $governanceParam2 = Get-Content -Path $governanceParam2Json -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceParam3Gate) {
    $governanceParam3 = Get-Content -Path $governanceParam3Json -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceMarketPolicyGate) {
    $governanceMarketPolicy = Get-Content -Path $governanceMarketPolicyJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceCouncilPolicyGate) {
    $governanceCouncilPolicy = Get-Content -Path $governanceCouncilPolicyJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceNegativeGate) {
    $governanceNegative = Get-Content -Path $governanceNegativeJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceAccessPolicyGate) {
    $governanceAccessPolicy = Get-Content -Path $governanceAccessPolicyJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceTokenEconomicsGate) {
    $governanceTokenEconomics = Get-Content -Path $governanceTokenEconomicsJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceTreasurySpendGate) {
    $governanceTreasurySpend = Get-Content -Path $governanceTreasurySpendJson -Raw | ConvertFrom-Json
}
if ($IncludeEconomicInfraDedicatedGate) {
    $economicInfraDedicated = Get-Content -Path $economicInfraDedicatedJson -Raw | ConvertFrom-Json
}
if ($IncludeEconomicServiceSurfaceGate) {
    $economicServiceSurface = Get-Content -Path $economicServiceSurfaceJson -Raw | ConvertFrom-Json
}
if ($IncludeOpsControlSurfaceGate) {
    $opsControlSurface = Get-Content -Path $opsControlSurfaceJson -Raw | ConvertFrom-Json
}
if ($IncludeMarketEngineTreasuryNegativeGate) {
    $marketEngineTreasuryNegative = Get-Content -Path $marketEngineTreasuryNegativeJson -Raw | ConvertFrom-Json
}
if ($IncludeForeignRateSourceGate) {
    $foreignRateSource = Get-Content -Path $foreignRateSourceJson -Raw | ConvertFrom-Json
}
if ($IncludeNavValuationSourceGate) {
    $navValuationSource = Get-Content -Path $navValuationSourceJson -Raw | ConvertFrom-Json
}
if ($IncludeDividendBalanceSourceGate) {
    $dividendBalanceSource = Get-Content -Path $dividendBalanceSourceJson -Raw | ConvertFrom-Json
}
if ($IncludeUnifiedAccountGate) {
    $unifiedAccount = Get-Content -Path $unifiedAccountJson -Raw | ConvertFrom-Json
}
if ($IncludeRpcExposureGate) {
    $rpcExposure = Get-Content -Path $rpcExposureJson -Raw | ConvertFrom-Json
}
if ($IncludeTestnetBootstrapGate) {
    $testnetBootstrap = Get-Content -Path $testnetBootstrapJson -Raw | ConvertFrom-Json
}
if ($IncludeUnjailCooldownGate) {
    $unjailCooldown = Get-Content -Path $unjailCooldownJson -Raw | ConvertFrom-Json
}
if ($IncludeAdapterStabilityGate) {
    $adapterStability = Get-Content -Path $adapterStabilityJson -Raw | ConvertFrom-Json
}
if ($IncludeVmRuntimeSplitGate) {
    $vmRuntimeSplit = Get-Content -Path $vmRuntimeSplitJson -Raw | ConvertFrom-Json
}
if ($IncludeEvmChainProfileSignalGate) {
    $evmChainProfileSignal = Get-Content -Path $evmChainProfileSignalJson -Raw | ConvertFrom-Json
}
if ($IncludeEvmTxTypeSignalGate) {
    $evmTxTypeSignal = Get-Content -Path $evmTxTypeSignalJson -Raw | ConvertFrom-Json
}
if ($IncludeOverlapRouterSignalGate) {
    $overlapRouterSignal = Get-Content -Path $overlapRouterSignalJson -Raw | ConvertFrom-Json
}
if ($IncludeEvmBackendCompareGate) {
    $evmBackendCompareEvm = Get-Content -Path $evmBackendCompareEvmJson -Raw | ConvertFrom-Json
    if ($EvmBackendCompareIncludePolygon) {
        $evmBackendComparePolygon = Get-Content -Path $evmBackendComparePolygonJson -Raw | ConvertFrom-Json
    }
    if ($EvmBackendCompareIncludeBnb) {
        $evmBackendCompareBnb = Get-Content -Path $evmBackendCompareBnbJson -Raw | ConvertFrom-Json
    }
    if ($EvmBackendCompareIncludeAvalanche) {
        $evmBackendCompareAvalanche = Get-Content -Path $evmBackendCompareAvalancheJson -Raw | ConvertFrom-Json
    }
}

$functionalPass = [bool]$functional.overall_pass
$blockAuditRootFfi = ""
$blockAuditRootLegacy = ""
$commitAuditRootFfi = ""
$commitAuditRootLegacy = ""
if ($null -ne $functional.block_output_signal -and $null -ne $functional.block_output_signal.ffi_v2) {
    if ($functional.block_output_signal.ffi_v2.PSObject.Properties.Name -contains "governance_chain_audit_root") {
        $blockAuditRootFfi = [string]$functional.block_output_signal.ffi_v2.governance_chain_audit_root
    }
}
if ($null -ne $functional.block_output_signal -and $null -ne $functional.block_output_signal.legacy_compat) {
    if ($functional.block_output_signal.legacy_compat.PSObject.Properties.Name -contains "governance_chain_audit_root") {
        $blockAuditRootLegacy = [string]$functional.block_output_signal.legacy_compat.governance_chain_audit_root
    }
}
if ($null -ne $functional.commit_output_signal -and $null -ne $functional.commit_output_signal.ffi_v2) {
    if ($functional.commit_output_signal.ffi_v2.PSObject.Properties.Name -contains "governance_chain_audit_root") {
        $commitAuditRootFfi = [string]$functional.commit_output_signal.ffi_v2.governance_chain_audit_root
    }
}
if ($null -ne $functional.commit_output_signal -and $null -ne $functional.commit_output_signal.legacy_compat) {
    if ($functional.commit_output_signal.legacy_compat.PSObject.Properties.Name -contains "governance_chain_audit_root") {
        $commitAuditRootLegacy = [string]$functional.commit_output_signal.legacy_compat.governance_chain_audit_root
    }
}
$functionalHasDirectAuditRootSignal = [bool](
    $functional.block_output_signal.available -and
    $functional.block_output_signal.pass -and
    $functional.commit_output_signal.available -and
    $functional.commit_output_signal.pass -and
    -not [string]::IsNullOrWhiteSpace($blockAuditRootFfi) -and
    -not [string]::IsNullOrWhiteSpace($blockAuditRootLegacy) -and
    -not [string]::IsNullOrWhiteSpace($commitAuditRootFfi) -and
    -not [string]::IsNullOrWhiteSpace($commitAuditRootLegacy)
)
$functionalDigestFallbackAuditRootParityPass = [bool](
    [bool]$functional.node_mode_consistency.pass -and
    [bool]$functional.state_root_consistency.pass -and
    -not [string]::IsNullOrWhiteSpace([string]$functional.state_root_consistency.proxy_digest)
)
$governanceChainAuditRootParityPass = if ($functionalHasDirectAuditRootSignal) {
    [bool](
        $blockAuditRootFfi -eq $blockAuditRootLegacy -and
        $commitAuditRootFfi -eq $commitAuditRootLegacy
    )
} else {
    $functionalDigestFallbackAuditRootParityPass
}
$performancePass = [bool]$performance.pass
if ($IncludeChainQueryRpcGate) {
    $chainQueryRpcPass = [bool]$chainQueryRpc.pass
} else {
    $chainQueryRpcPass = $true
}
if ($IncludeGovernanceRpcGate) {
    $governanceRpcPass = [bool]$governanceRpc.pass
    $governanceRpcAuditPersistPass = [bool]$governanceRpc.audit_persist_ok
    $governanceRpcSignatureSchemeRejectPass = [bool]$governanceRpc.sign_unsupported_scheme_reject_ok
    $governanceRpcVoteVerifierStartupPass = [bool]$governanceRpc.vote_verifier_startup_ok
    $governanceRpcVoteVerifierStagedRejectPass = [bool]$governanceRpc.vote_verifier_staged_reject_ok
    $governanceRpcVoteVerifierExecutePass = [bool]$governanceRpc.execute_vote_verifier_ok
    $governanceRpcChainAuditPass = [bool]$governanceRpc.chain_audit_ok
    $governanceRpcChainAuditPersistPass = [bool]$governanceRpc.chain_audit_persist_ok
    $governanceRpcChainAuditRestartPass = [bool]$governanceRpc.chain_audit_restart_ok
    $governanceRpcChainAuditExecuteVerifierPass = [bool]$governanceRpc.chain_audit_has_execute_applied_verifier
    $governanceRpcChainAuditPersistExecuteVerifierPass = [bool]$governanceRpc.chain_audit_persist_has_execute_applied_verifier
    $governanceRpcChainAuditRestartExecuteVerifierPass = [bool]$governanceRpc.chain_audit_restart_has_execute_applied_verifier
    $governanceRpcChainAuditExecuteVerifierProofPass = [bool](
        $governanceRpcChainAuditExecuteVerifierPass -and
        $governanceRpcChainAuditPersistExecuteVerifierPass -and
        $governanceRpcChainAuditRestartExecuteVerifierPass
    )
    $governanceRpcPolicyChainAuditConsistencyPass = [bool]$governanceRpc.policy_chain_audit_consistency_ok
    $governanceRpcChainAuditRootPass = [bool]$governanceRpc.chain_audit_root_ok
    $governanceRpcChainAuditPersistRootPass = [bool]$governanceRpc.chain_audit_persist_root_ok
    $governanceRpcChainAuditRestartRootPass = [bool]$governanceRpc.chain_audit_restart_root_ok
    $governanceRpcChainAuditRootProofPass = [bool](
        $governanceRpcPolicyChainAuditConsistencyPass -and
        $governanceRpcChainAuditRootPass -and
        $governanceRpcChainAuditPersistRootPass -and
        $governanceRpcChainAuditRestartRootPass
    )
    $governanceRpcPass = [bool](
        $governanceRpcPass -and
        $governanceRpcAuditPersistPass -and
        $governanceRpcSignatureSchemeRejectPass -and
        $governanceRpcVoteVerifierStartupPass -and
        $governanceRpcVoteVerifierStagedRejectPass -and
        $governanceRpcVoteVerifierExecutePass -and
        $governanceRpcChainAuditPass -and
        $governanceRpcChainAuditPersistPass -and
        $governanceRpcChainAuditRestartPass -and
        $governanceRpcChainAuditExecuteVerifierProofPass -and
        $governanceRpcChainAuditRootProofPass
    )
} else {
    $governanceRpcPass = $true
    $governanceRpcAuditPersistPass = $true
    $governanceRpcSignatureSchemeRejectPass = $true
    $governanceRpcVoteVerifierStartupPass = $true
    $governanceRpcVoteVerifierStagedRejectPass = $true
    $governanceRpcVoteVerifierExecutePass = $true
    $governanceRpcChainAuditPass = $true
    $governanceRpcChainAuditPersistPass = $true
    $governanceRpcChainAuditRestartPass = $true
    $governanceRpcChainAuditExecuteVerifierPass = $true
    $governanceRpcChainAuditPersistExecuteVerifierPass = $true
    $governanceRpcChainAuditRestartExecuteVerifierPass = $true
    $governanceRpcChainAuditExecuteVerifierProofPass = $true
    $governanceRpcPolicyChainAuditConsistencyPass = $true
    $governanceRpcChainAuditRootPass = $true
    $governanceRpcChainAuditPersistRootPass = $true
    $governanceRpcChainAuditRestartRootPass = $true
    $governanceRpcChainAuditRootProofPass = $true
}
if ($IncludeGovernanceRpcMldsaFfiGate) {
    $governanceRpcMldsaFfiPass = [bool]$governanceRpcMldsaFfi.pass
    $governanceRpcMldsaFfiStartupPass = [bool]$governanceRpcMldsaFfi.vote_verifier_startup_ok
    $governanceRpcMldsaFfiPass = [bool](
        $governanceRpcMldsaFfiPass -and
        $governanceRpcMldsaFfiStartupPass
    )
} else {
    $governanceRpcMldsaFfiPass = $true
    $governanceRpcMldsaFfiStartupPass = $true
}
if ($IncludeHeaderSyncGate) {
    $headerSyncPass = [bool]$headerSync.pass
} else {
    $headerSyncPass = $true
}
if ($IncludeFastStateSyncGate) {
    $fastStateSyncPass = [bool]$fastStateSync.pass
} else {
    $fastStateSyncPass = $true
}
if ($IncludeNetworkDosGate) {
    $networkDosPass = [bool]$networkDos.pass
} else {
    $networkDosPass = $true
}
if ($IncludePacemakerFailoverGate) {
    $pacemakerFailoverPass = [bool]$pacemakerFailover.pass
} else {
    $pacemakerFailoverPass = $true
}
if ($IncludeSlashGovernanceGate) {
    $slashGovernancePass = [bool]$slashGovernance.pass
} else {
    $slashGovernancePass = $true
}
if ($IncludeSlashPolicyExternalGate) {
    $slashPolicyExternalPass = [bool]$slashPolicyExternal.pass
} else {
    $slashPolicyExternalPass = $true
}
if ($IncludeGovernanceHookGate) {
    $governanceHookPass = [bool]$governanceHook.pass
} else {
    $governanceHookPass = $true
}
if ($IncludeGovernanceExecutionGate) {
    $governanceExecutionPass = [bool]$governanceExecution.pass
} else {
    $governanceExecutionPass = $true
}
if ($IncludeGovernanceParam2Gate) {
    $governanceParam2Pass = [bool]$governanceParam2.pass
} else {
    $governanceParam2Pass = $true
}
if ($IncludeGovernanceParam3Gate) {
    $governanceParam3Pass = [bool]$governanceParam3.pass
} else {
    $governanceParam3Pass = $true
}
if ($IncludeGovernanceMarketPolicyGate) {
    $governanceMarketPolicyPass = [bool]$governanceMarketPolicy.pass
    $governanceMarketPolicyEnginePass = [bool]$governanceMarketPolicy.engine_output_pass
    $governanceMarketPolicyTreasuryPass = [bool]$governanceMarketPolicy.treasury_output_pass
    $governanceMarketPolicyOrchestrationPass = [bool]$governanceMarketPolicy.orchestration_output_pass
    $governanceMarketPolicyDividendPass = [bool]$governanceMarketPolicy.dividend_output_pass
    $governanceMarketPolicyForeignPass = [bool]$governanceMarketPolicy.foreign_payment_output_pass
    $governanceMarketPolicyPass = [bool](
        $governanceMarketPolicyPass -and
        $governanceMarketPolicyEnginePass -and
        $governanceMarketPolicyTreasuryPass -and
        $governanceMarketPolicyOrchestrationPass -and
        $governanceMarketPolicyDividendPass -and
        $governanceMarketPolicyForeignPass
    )
} else {
    $governanceMarketPolicyPass = $true
    $governanceMarketPolicyEnginePass = $true
    $governanceMarketPolicyTreasuryPass = $true
    $governanceMarketPolicyOrchestrationPass = $true
    $governanceMarketPolicyDividendPass = $true
    $governanceMarketPolicyForeignPass = $true
}
if ($IncludeGovernanceCouncilPolicyGate) {
    $governanceCouncilPolicyPass = [bool]$governanceCouncilPolicy.pass
} else {
    $governanceCouncilPolicyPass = $true
}
if ($IncludeGovernanceNegativeGate) {
    $governanceNegativePass = [bool]$governanceNegative.pass
} else {
    $governanceNegativePass = $true
}
if ($IncludeGovernanceAccessPolicyGate) {
    $governanceAccessPolicyPass = [bool]$governanceAccessPolicy.pass
} else {
    $governanceAccessPolicyPass = $true
}
if ($IncludeGovernanceTokenEconomicsGate) {
    $governanceTokenEconomicsPass = [bool]$governanceTokenEconomics.pass
} else {
    $governanceTokenEconomicsPass = $true
}
if ($IncludeGovernanceTreasurySpendGate) {
    $governanceTreasurySpendPass = [bool]$governanceTreasurySpend.pass
} else {
    $governanceTreasurySpendPass = $true
}
if ($IncludeEconomicInfraDedicatedGate) {
    $economicInfraDedicatedPass = [bool]$economicInfraDedicated.pass
    $economicInfraDedicatedTokenPass = [bool]$economicInfraDedicated.token_system_pass
    $economicInfraDedicatedAmmPass = [bool]$economicInfraDedicated.amm_pass
    $economicInfraDedicatedNavPass = [bool]$economicInfraDedicated.nav_redemption_pass
    $economicInfraDedicatedCdpPass = [bool]$economicInfraDedicated.cdp_pass
    $economicInfraDedicatedBondPass = [bool]$economicInfraDedicated.bond_pass
    $economicInfraDedicatedTreasuryPass = [bool]$economicInfraDedicated.treasury_pass
    $economicInfraDedicatedGovernancePass = [bool]$economicInfraDedicated.governance_system_pass
    $economicInfraDedicatedDividendPass = [bool]$economicInfraDedicated.dividend_pool_pass
    $economicInfraDedicatedForeignPass = [bool]$economicInfraDedicated.foreign_payment_pass
    $economicInfraDedicatedPass = [bool](
        $economicInfraDedicatedPass -and
        $economicInfraDedicatedTokenPass -and
        $economicInfraDedicatedAmmPass -and
        $economicInfraDedicatedNavPass -and
        $economicInfraDedicatedCdpPass -and
        $economicInfraDedicatedBondPass -and
        $economicInfraDedicatedTreasuryPass -and
        $economicInfraDedicatedGovernancePass -and
        $economicInfraDedicatedDividendPass -and
        $economicInfraDedicatedForeignPass
    )
} else {
    $economicInfraDedicatedPass = $true
    $economicInfraDedicatedTokenPass = $true
    $economicInfraDedicatedAmmPass = $true
    $economicInfraDedicatedNavPass = $true
    $economicInfraDedicatedCdpPass = $true
    $economicInfraDedicatedBondPass = $true
    $economicInfraDedicatedTreasuryPass = $true
    $economicInfraDedicatedGovernancePass = $true
    $economicInfraDedicatedDividendPass = $true
    $economicInfraDedicatedForeignPass = $true
}
if ($IncludeEconomicServiceSurfaceGate) {
    $economicServiceSurfacePass = [bool]$economicServiceSurface.pass
    $economicServiceSurfaceTokenPass = [bool]$economicServiceSurface.token_system_pass
    $economicServiceSurfaceAmmPass = [bool]$economicServiceSurface.amm_pass
    $economicServiceSurfaceCdpPass = [bool]$economicServiceSurface.cdp_pass
    $economicServiceSurfaceBondPass = [bool]$economicServiceSurface.bond_pass
    $economicServiceSurfaceNavPass = [bool]$economicServiceSurface.nav_redemption_pass
    $economicServiceSurfaceTreasuryPass = [bool]$economicServiceSurface.treasury_pass
    $economicServiceSurfaceGovernancePass = [bool]$economicServiceSurface.governance_system_pass
    $economicServiceSurfaceDividendPass = [bool]$economicServiceSurface.dividend_pool_pass
    $economicServiceSurfaceForeignPass = [bool]$economicServiceSurface.foreign_payment_pass
    $economicServiceSurfacePass = [bool](
        $economicServiceSurfacePass -and
        $economicServiceSurfaceTokenPass -and
        $economicServiceSurfaceAmmPass -and
        $economicServiceSurfaceCdpPass -and
        $economicServiceSurfaceBondPass -and
        $economicServiceSurfaceNavPass -and
        $economicServiceSurfaceTreasuryPass -and
        $economicServiceSurfaceGovernancePass -and
        $economicServiceSurfaceDividendPass -and
        $economicServiceSurfaceForeignPass
    )
} else {
    $economicServiceSurfacePass = $true
    $economicServiceSurfaceTokenPass = $true
    $economicServiceSurfaceAmmPass = $true
    $economicServiceSurfaceCdpPass = $true
    $economicServiceSurfaceBondPass = $true
    $economicServiceSurfaceNavPass = $true
    $economicServiceSurfaceTreasuryPass = $true
    $economicServiceSurfaceGovernancePass = $true
    $economicServiceSurfaceDividendPass = $true
    $economicServiceSurfaceForeignPass = $true
}
if ($IncludeOpsControlSurfaceGate) {
    $opsControlSurfacePass = [bool]$opsControlSurface.pass
    $opsControlSurfaceRateLimitPass = [bool]$opsControlSurface.rate_limit_pass
    $opsControlSurfaceCircuitBreakerPass = [bool]$opsControlSurface.circuit_breaker_pass
    $opsControlSurfaceQuotaPass = [bool]$opsControlSurface.quota_pass
    $opsControlSurfaceAlertFieldPass = [bool]$opsControlSurface.alert_field_pass
    $opsControlSurfaceAuditFieldPass = [bool]$opsControlSurface.audit_field_pass
    $opsControlSurfacePass = [bool](
        $opsControlSurfacePass -and
        $opsControlSurfaceRateLimitPass -and
        $opsControlSurfaceCircuitBreakerPass -and
        $opsControlSurfaceQuotaPass -and
        $opsControlSurfaceAlertFieldPass -and
        $opsControlSurfaceAuditFieldPass
    )
} else {
    $opsControlSurfacePass = $true
    $opsControlSurfaceRateLimitPass = $true
    $opsControlSurfaceCircuitBreakerPass = $true
    $opsControlSurfaceQuotaPass = $true
    $opsControlSurfaceAlertFieldPass = $true
    $opsControlSurfaceAuditFieldPass = $true
}
if ($IncludeMarketEngineTreasuryNegativeGate) {
    $marketEngineTreasuryNegativePass = [bool]$marketEngineTreasuryNegative.pass
} else {
    $marketEngineTreasuryNegativePass = $true
}
if ($IncludeForeignRateSourceGate) {
    $foreignRateSourcePass = [bool]$foreignRateSource.pass
} else {
    $foreignRateSourcePass = $true
}
if ($IncludeNavValuationSourceGate) {
    $navValuationSourcePass = [bool]$navValuationSource.pass
} else {
    $navValuationSourcePass = $true
}
if ($IncludeDividendBalanceSourceGate) {
    $dividendBalanceSourcePass = [bool]$dividendBalanceSource.pass
} else {
    $dividendBalanceSourcePass = $true
}
if ($IncludeUnifiedAccountGate) {
    $unifiedAccountPass = [bool]$unifiedAccount.pass
} else {
    $unifiedAccountPass = $true
}
if ($IncludeRpcExposureGate) {
    $rpcExposurePass = [bool]$rpcExposure.pass
} else {
    $rpcExposurePass = $true
}
if ($IncludeTestnetBootstrapGate) {
    $testnetBootstrapPass = [bool]$testnetBootstrap.pass
} else {
    $testnetBootstrapPass = $true
}
if ($IncludeUnjailCooldownGate) {
    $unjailCooldownPass = [bool]$unjailCooldown.pass
} else {
    $unjailCooldownPass = $true
}
if ($IncludeAdapterStabilityGate) {
    $adapterStabilityPass = [bool]$adapterStability.pass
} else {
    $adapterStabilityPass = $true
}
if ($IncludeVmRuntimeSplitGate) {
    $vmRuntimeSplitPass = [bool]$vmRuntimeSplit.pass
} else {
    $vmRuntimeSplitPass = $true
}
if ($IncludeEvmChainProfileSignalGate) {
    $evmChainProfileSignalPass = [bool]$evmChainProfileSignal.pass
} else {
    $evmChainProfileSignalPass = $true
}
if ($IncludeEvmTxTypeSignalGate) {
    $evmTxTypeSignalPass = [bool]$evmTxTypeSignal.pass
} else {
    $evmTxTypeSignalPass = $true
}
if ($IncludeOverlapRouterSignalGate) {
    $overlapRouterSignalPass = [bool]$overlapRouterSignal.pass
} else {
    $overlapRouterSignalPass = $true
}
if ($IncludeEvmBackendCompareGate) {
    $evmBackendCompareEvmPass = [bool]$evmBackendCompareEvm.pass
    if ($EvmBackendCompareIncludePolygon) {
        $evmBackendComparePolygonPass = [bool]$evmBackendComparePolygon.pass
    } else {
        $evmBackendComparePolygonPass = $true
    }
    if ($EvmBackendCompareIncludeBnb) {
        $evmBackendCompareBnbPass = [bool]$evmBackendCompareBnb.pass
    } else {
        $evmBackendCompareBnbPass = $true
    }
    if ($EvmBackendCompareIncludeAvalanche) {
        $evmBackendCompareAvalanchePass = [bool]$evmBackendCompareAvalanche.pass
    } else {
        $evmBackendCompareAvalanchePass = $true
    }
    $evmBackendComparePass = [bool](
        $evmBackendCompareEvmPass -and
        $evmBackendComparePolygonPass -and
        $evmBackendCompareBnbPass -and
        $evmBackendCompareAvalanchePass
    )
} else {
    $evmBackendCompareEvmPass = $true
    $evmBackendComparePolygonPass = $true
    $evmBackendCompareBnbPass = $true
    $evmBackendCompareAvalanchePass = $true
    $evmBackendComparePass = $true
}
$overallPass = ($functionalPass -and $governanceChainAuditRootParityPass -and $performancePass -and $chainQueryRpcPass -and $governanceRpcPass -and $governanceRpcAuditPersistPass -and $governanceRpcSignatureSchemeRejectPass -and $governanceRpcVoteVerifierStartupPass -and $governanceRpcVoteVerifierStagedRejectPass -and $governanceRpcVoteVerifierExecutePass -and $governanceRpcChainAuditExecuteVerifierProofPass -and $governanceRpcChainAuditRootProofPass -and $governanceRpcMldsaFfiPass -and $governanceRpcMldsaFfiStartupPass -and $headerSyncPass -and $fastStateSyncPass -and $networkDosPass -and $pacemakerFailoverPass -and $slashGovernancePass -and $slashPolicyExternalPass -and $governanceHookPass -and $governanceExecutionPass -and $governanceParam2Pass -and $governanceParam3Pass -and $governanceMarketPolicyPass -and $governanceMarketPolicyEnginePass -and $governanceMarketPolicyTreasuryPass -and $governanceMarketPolicyOrchestrationPass -and $governanceMarketPolicyDividendPass -and $governanceMarketPolicyForeignPass -and $governanceCouncilPolicyPass -and $governanceNegativePass -and $governanceAccessPolicyPass -and $governanceTokenEconomicsPass -and $governanceTreasurySpendPass -and $economicInfraDedicatedPass -and $economicInfraDedicatedTokenPass -and $economicInfraDedicatedAmmPass -and $economicInfraDedicatedNavPass -and $economicInfraDedicatedCdpPass -and $economicInfraDedicatedBondPass -and $economicInfraDedicatedTreasuryPass -and $economicInfraDedicatedGovernancePass -and $economicInfraDedicatedDividendPass -and $economicInfraDedicatedForeignPass -and $economicServiceSurfacePass -and $economicServiceSurfaceTokenPass -and $economicServiceSurfaceAmmPass -and $economicServiceSurfaceCdpPass -and $economicServiceSurfaceBondPass -and $economicServiceSurfaceNavPass -and $economicServiceSurfaceTreasuryPass -and $economicServiceSurfaceGovernancePass -and $economicServiceSurfaceDividendPass -and $economicServiceSurfaceForeignPass -and $opsControlSurfacePass -and $opsControlSurfaceRateLimitPass -and $opsControlSurfaceCircuitBreakerPass -and $opsControlSurfaceQuotaPass -and $opsControlSurfaceAlertFieldPass -and $opsControlSurfaceAuditFieldPass -and $marketEngineTreasuryNegativePass -and $foreignRateSourcePass -and $navValuationSourcePass -and $dividendBalanceSourcePass -and $unifiedAccountPass -and $rpcExposurePass -and $testnetBootstrapPass -and $unjailCooldownPass -and $adapterStabilityPass -and $vmRuntimeSplitPass -and $evmChainProfileSignalPass -and $evmTxTypeSignalPass -and $overlapRouterSignalPass -and $evmBackendComparePass)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    profile_name = $profileName
    full_snapshot_profile = [bool]($FullSnapshotProfile -or $FullSnapshotProfileV2 -or $FullSnapshotProfileGA)
    full_snapshot_profile_v2 = [bool]$FullSnapshotProfileV2
    overall_pass = $overallPass
    functional_pass = $functionalPass
    governance_chain_audit_root_parity_pass = $governanceChainAuditRootParityPass
    performance_gate_enabled = $IncludePerformanceGate
    performance_pass = $performancePass
    chain_query_rpc_gate_enabled = $IncludeChainQueryRpcGate
    chain_query_rpc_pass = $chainQueryRpcPass
    governance_rpc_gate_enabled = $IncludeGovernanceRpcGate
    governance_rpc_pass = $governanceRpcPass
    governance_rpc_audit_persist_pass = $governanceRpcAuditPersistPass
    governance_rpc_signature_scheme_reject_pass = $governanceRpcSignatureSchemeRejectPass
    governance_rpc_vote_verifier_startup_pass = $governanceRpcVoteVerifierStartupPass
    governance_rpc_vote_verifier_staged_reject_pass = $governanceRpcVoteVerifierStagedRejectPass
    governance_rpc_vote_verifier_execute_pass = $governanceRpcVoteVerifierExecutePass
    governance_rpc_chain_audit_pass = $governanceRpcChainAuditPass
    governance_rpc_chain_audit_persist_pass = $governanceRpcChainAuditPersistPass
    governance_rpc_chain_audit_restart_pass = $governanceRpcChainAuditRestartPass
    governance_rpc_chain_audit_execute_verifier_pass = $governanceRpcChainAuditExecuteVerifierPass
    governance_rpc_chain_audit_persist_execute_verifier_pass = $governanceRpcChainAuditPersistExecuteVerifierPass
    governance_rpc_chain_audit_restart_execute_verifier_pass = $governanceRpcChainAuditRestartExecuteVerifierPass
    governance_rpc_chain_audit_execute_verifier_proof_pass = $governanceRpcChainAuditExecuteVerifierProofPass
    governance_rpc_policy_chain_audit_consistency_pass = $governanceRpcPolicyChainAuditConsistencyPass
    governance_rpc_chain_audit_root_pass = $governanceRpcChainAuditRootPass
    governance_rpc_chain_audit_persist_root_pass = $governanceRpcChainAuditPersistRootPass
    governance_rpc_chain_audit_restart_root_pass = $governanceRpcChainAuditRestartRootPass
    governance_rpc_chain_audit_root_proof_pass = $governanceRpcChainAuditRootProofPass
    governance_rpc_mldsa_ffi_gate_enabled = $IncludeGovernanceRpcMldsaFfiGate
    governance_rpc_mldsa_ffi_pass = $governanceRpcMldsaFfiPass
    governance_rpc_mldsa_ffi_startup_pass = $governanceRpcMldsaFfiStartupPass
    header_sync_gate_enabled = $IncludeHeaderSyncGate
    header_sync_pass = $headerSyncPass
    fast_state_sync_gate_enabled = $IncludeFastStateSyncGate
    fast_state_sync_pass = $fastStateSyncPass
    network_dos_gate_enabled = $IncludeNetworkDosGate
    network_dos_pass = $networkDosPass
    pacemaker_failover_gate_enabled = $IncludePacemakerFailoverGate
    pacemaker_failover_pass = $pacemakerFailoverPass
    slash_governance_gate_enabled = $IncludeSlashGovernanceGate
    slash_governance_pass = $slashGovernancePass
    slash_policy_external_gate_enabled = $IncludeSlashPolicyExternalGate
    slash_policy_external_pass = $slashPolicyExternalPass
    governance_hook_gate_enabled = $IncludeGovernanceHookGate
    governance_hook_pass = $governanceHookPass
    governance_execution_gate_enabled = $IncludeGovernanceExecutionGate
    governance_execution_pass = $governanceExecutionPass
    governance_param2_gate_enabled = $IncludeGovernanceParam2Gate
    governance_param2_pass = $governanceParam2Pass
    governance_param3_gate_enabled = $IncludeGovernanceParam3Gate
    governance_param3_pass = $governanceParam3Pass
    governance_market_policy_gate_enabled = $IncludeGovernanceMarketPolicyGate
    governance_market_policy_pass = $governanceMarketPolicyPass
    governance_market_policy_engine_pass = $governanceMarketPolicyEnginePass
    governance_market_policy_treasury_pass = $governanceMarketPolicyTreasuryPass
    governance_market_policy_orchestration_pass = $governanceMarketPolicyOrchestrationPass
    governance_market_policy_dividend_pass = $governanceMarketPolicyDividendPass
    governance_market_policy_foreign_payment_pass = $governanceMarketPolicyForeignPass
    governance_council_policy_gate_enabled = $IncludeGovernanceCouncilPolicyGate
    governance_council_policy_pass = $governanceCouncilPolicyPass
    governance_negative_gate_enabled = $IncludeGovernanceNegativeGate
    governance_negative_pass = $governanceNegativePass
    governance_access_policy_gate_enabled = $IncludeGovernanceAccessPolicyGate
    governance_access_policy_pass = $governanceAccessPolicyPass
    governance_token_economics_gate_enabled = $IncludeGovernanceTokenEconomicsGate
    governance_token_economics_pass = $governanceTokenEconomicsPass
    governance_treasury_spend_gate_enabled = $IncludeGovernanceTreasurySpendGate
    governance_treasury_spend_pass = $governanceTreasurySpendPass
    economic_infra_dedicated_gate_enabled = $IncludeEconomicInfraDedicatedGate
    economic_infra_dedicated_pass = $economicInfraDedicatedPass
    economic_infra_dedicated_token_system_pass = $economicInfraDedicatedTokenPass
    economic_infra_dedicated_amm_pass = $economicInfraDedicatedAmmPass
    economic_infra_dedicated_nav_redemption_pass = $economicInfraDedicatedNavPass
    economic_infra_dedicated_cdp_pass = $economicInfraDedicatedCdpPass
    economic_infra_dedicated_bond_pass = $economicInfraDedicatedBondPass
    economic_infra_dedicated_treasury_pass = $economicInfraDedicatedTreasuryPass
    economic_infra_dedicated_governance_system_pass = $economicInfraDedicatedGovernancePass
    economic_infra_dedicated_dividend_pool_pass = $economicInfraDedicatedDividendPass
    economic_infra_dedicated_foreign_payment_pass = $economicInfraDedicatedForeignPass
    economic_service_surface_gate_enabled = $IncludeEconomicServiceSurfaceGate
    economic_service_surface_pass = $economicServiceSurfacePass
    economic_service_surface_token_system_pass = $economicServiceSurfaceTokenPass
    economic_service_surface_amm_pass = $economicServiceSurfaceAmmPass
    economic_service_surface_cdp_pass = $economicServiceSurfaceCdpPass
    economic_service_surface_bond_pass = $economicServiceSurfaceBondPass
    economic_service_surface_nav_redemption_pass = $economicServiceSurfaceNavPass
    economic_service_surface_treasury_pass = $economicServiceSurfaceTreasuryPass
    economic_service_surface_governance_system_pass = $economicServiceSurfaceGovernancePass
    economic_service_surface_dividend_pool_pass = $economicServiceSurfaceDividendPass
    economic_service_surface_foreign_payment_pass = $economicServiceSurfaceForeignPass
    ops_control_surface_gate_enabled = $IncludeOpsControlSurfaceGate
    ops_control_surface_pass = $opsControlSurfacePass
    ops_control_surface_rate_limit_pass = $opsControlSurfaceRateLimitPass
    ops_control_surface_circuit_breaker_pass = $opsControlSurfaceCircuitBreakerPass
    ops_control_surface_quota_pass = $opsControlSurfaceQuotaPass
    ops_control_surface_alert_field_pass = $opsControlSurfaceAlertFieldPass
    ops_control_surface_audit_field_pass = $opsControlSurfaceAuditFieldPass
    market_engine_treasury_negative_gate_enabled = $IncludeMarketEngineTreasuryNegativeGate
    market_engine_treasury_negative_pass = $marketEngineTreasuryNegativePass
    foreign_rate_source_gate_enabled = $IncludeForeignRateSourceGate
    foreign_rate_source_pass = $foreignRateSourcePass
    nav_valuation_source_gate_enabled = $IncludeNavValuationSourceGate
    nav_valuation_source_pass = $navValuationSourcePass
    dividend_balance_source_gate_enabled = $IncludeDividendBalanceSourceGate
    dividend_balance_source_pass = $dividendBalanceSourcePass
    unified_account_gate_enabled = $IncludeUnifiedAccountGate
    unified_account_pass = $unifiedAccountPass
    rpc_exposure_gate_enabled = $IncludeRpcExposureGate
    rpc_exposure_pass = $rpcExposurePass
    testnet_bootstrap_gate_enabled = $IncludeTestnetBootstrapGate
    testnet_bootstrap_pass = $testnetBootstrapPass
    unjail_cooldown_gate_enabled = $IncludeUnjailCooldownGate
    unjail_cooldown_pass = $unjailCooldownPass
    adapter_stability_enabled = $IncludeAdapterStabilityGate
    adapter_stability_pass = $adapterStabilityPass
    vm_runtime_split_gate_enabled = $IncludeVmRuntimeSplitGate
    vm_runtime_split_pass = $vmRuntimeSplitPass
    evm_chain_profile_signal_gate_enabled = $IncludeEvmChainProfileSignalGate
    evm_chain_profile_signal_pass = $evmChainProfileSignalPass
    evm_tx_type_signal_gate_enabled = $IncludeEvmTxTypeSignalGate
    evm_tx_type_signal_pass = $evmTxTypeSignalPass
    overlap_router_signal_gate_enabled = $IncludeOverlapRouterSignalGate
    overlap_router_signal_pass = $overlapRouterSignalPass
    evm_backend_compare_gate_enabled = $IncludeEvmBackendCompareGate
    evm_backend_compare_include_polygon = if ($IncludeEvmBackendCompareGate) { $EvmBackendCompareIncludePolygon } else { $false }
    evm_backend_compare_include_bnb = if ($IncludeEvmBackendCompareGate) { $EvmBackendCompareIncludeBnb } else { $false }
    evm_backend_compare_include_avalanche = if ($IncludeEvmBackendCompareGate) { $EvmBackendCompareIncludeAvalanche } else { $false }
    evm_backend_compare_evm_pass = $evmBackendCompareEvmPass
    evm_backend_compare_polygon_pass = $evmBackendComparePolygonPass
    evm_backend_compare_bnb_pass = $evmBackendCompareBnbPass
    evm_backend_compare_avalanche_pass = $evmBackendCompareAvalanchePass
    evm_backend_compare_pass = $evmBackendComparePass
    functional_report_json = $functionalJson
    performance_report_json = if ($IncludePerformanceGate) { $performanceJson } else { "" }
    chain_query_rpc_report_json = if ($IncludeChainQueryRpcGate) { $chainQueryRpcJson } else { "" }
    governance_rpc_report_json = if ($IncludeGovernanceRpcGate) { $governanceRpcJson } else { "" }
    governance_rpc_mldsa_ffi_report_json = if ($IncludeGovernanceRpcMldsaFfiGate) { $governanceRpcMldsaFfiJson } else { "" }
    header_sync_report_json = if ($IncludeHeaderSyncGate) { $headerSyncJson } else { "" }
    fast_state_sync_report_json = if ($IncludeFastStateSyncGate) { $fastStateSyncJson } else { "" }
    network_dos_report_json = if ($IncludeNetworkDosGate) { $networkDosJson } else { "" }
    pacemaker_failover_report_json = if ($IncludePacemakerFailoverGate) { $pacemakerFailoverJson } else { "" }
    slash_governance_report_json = if ($IncludeSlashGovernanceGate) { $slashGovernanceJson } else { "" }
    slash_policy_external_report_json = if ($IncludeSlashPolicyExternalGate) { $slashPolicyExternalJson } else { "" }
    governance_hook_report_json = if ($IncludeGovernanceHookGate) { $governanceHookJson } else { "" }
    governance_execution_report_json = if ($IncludeGovernanceExecutionGate) { $governanceExecutionJson } else { "" }
    governance_param2_report_json = if ($IncludeGovernanceParam2Gate) { $governanceParam2Json } else { "" }
    governance_param3_report_json = if ($IncludeGovernanceParam3Gate) { $governanceParam3Json } else { "" }
    governance_market_policy_report_json = if ($IncludeGovernanceMarketPolicyGate) { $governanceMarketPolicyJson } else { "" }
    governance_council_policy_report_json = if ($IncludeGovernanceCouncilPolicyGate) { $governanceCouncilPolicyJson } else { "" }
    governance_negative_report_json = if ($IncludeGovernanceNegativeGate) { $governanceNegativeJson } else { "" }
    governance_access_policy_report_json = if ($IncludeGovernanceAccessPolicyGate) { $governanceAccessPolicyJson } else { "" }
    governance_token_economics_report_json = if ($IncludeGovernanceTokenEconomicsGate) { $governanceTokenEconomicsJson } else { "" }
    governance_treasury_spend_report_json = if ($IncludeGovernanceTreasurySpendGate) { $governanceTreasurySpendJson } else { "" }
    economic_infra_dedicated_report_json = if ($IncludeEconomicInfraDedicatedGate) { $economicInfraDedicatedJson } else { "" }
    economic_service_surface_report_json = if ($IncludeEconomicServiceSurfaceGate) { $economicServiceSurfaceJson } else { "" }
    ops_control_surface_report_json = if ($IncludeOpsControlSurfaceGate) { $opsControlSurfaceJson } else { "" }
    market_engine_treasury_negative_report_json = if ($IncludeMarketEngineTreasuryNegativeGate) { $marketEngineTreasuryNegativeJson } else { "" }
    foreign_rate_source_report_json = if ($IncludeForeignRateSourceGate) { $foreignRateSourceJson } else { "" }
    nav_valuation_source_report_json = if ($IncludeNavValuationSourceGate) { $navValuationSourceJson } else { "" }
    dividend_balance_source_report_json = if ($IncludeDividendBalanceSourceGate) { $dividendBalanceSourceJson } else { "" }
    unified_account_report_json = if ($IncludeUnifiedAccountGate) { $unifiedAccountJson } else { "" }
    rpc_exposure_report_json = if ($IncludeRpcExposureGate) { $rpcExposureJson } else { "" }
    testnet_bootstrap_report_json = if ($IncludeTestnetBootstrapGate) { $testnetBootstrapJson } else { "" }
    unjail_cooldown_report_json = if ($IncludeUnjailCooldownGate) { $unjailCooldownJson } else { "" }
    adapter_stability_report_json = if ($IncludeAdapterStabilityGate) { $adapterStabilityJson } else { "" }
    vm_runtime_split_report_json = if ($IncludeVmRuntimeSplitGate) { $vmRuntimeSplitJson } else { "" }
    evm_chain_profile_signal_report_json = if ($IncludeEvmChainProfileSignalGate) { $evmChainProfileSignalJson } else { "" }
    evm_tx_type_signal_report_json = if ($IncludeEvmTxTypeSignalGate) { $evmTxTypeSignalJson } else { "" }
    overlap_router_signal_report_json = if ($IncludeOverlapRouterSignalGate) { $overlapRouterSignalJson } else { "" }
    evm_backend_compare_evm_report_json = if ($IncludeEvmBackendCompareGate) { $evmBackendCompareEvmJson } else { "" }
    evm_backend_compare_polygon_report_json = if ($IncludeEvmBackendCompareGate -and $EvmBackendCompareIncludePolygon) { $evmBackendComparePolygonJson } else { "" }
    evm_backend_compare_bnb_report_json = if ($IncludeEvmBackendCompareGate -and $EvmBackendCompareIncludeBnb) { $evmBackendCompareBnbJson } else { "" }
    evm_backend_compare_avalanche_report_json = if ($IncludeEvmBackendCompareGate -and $EvmBackendCompareIncludeAvalanche) { $evmBackendCompareAvalancheJson } else { "" }
    performance_runs = if ($IncludePerformanceGate) { $PerformanceRuns } else { 0 }
    chain_query_rpc_expected_requests = if ($IncludeChainQueryRpcGate) { $ChainQueryRpcExpectedRequests } else { 0 }
    chain_query_rpc_bind = if ($IncludeChainQueryRpcGate) { $ChainQueryRpcBind } else { "" }
    governance_rpc_expected_requests = if ($IncludeGovernanceRpcGate) { $GovernanceRpcExpectedRequests } else { 0 }
    governance_rpc_bind = if ($IncludeGovernanceRpcGate) { $GovernanceRpcBind } else { "" }
    governance_rpc_mldsa_ffi_expected_requests = if ($IncludeGovernanceRpcMldsaFfiGate) { $GovernanceRpcMldsaFfiExpectedRequests } else { 0 }
    governance_rpc_mldsa_ffi_bind = if ($IncludeGovernanceRpcMldsaFfiGate) { $GovernanceRpcMldsaFfiBind } else { "" }
    rpc_exposure_public_bind = if ($IncludeRpcExposureGate) { $RpcExposurePublicBind } else { "" }
    rpc_exposure_gov_bind = if ($IncludeRpcExposureGate) { $RpcExposureGovBind } else { "" }
    pacemaker_failover_nodes = if ($IncludePacemakerFailoverGate) { $PacemakerFailoverNodes } else { 0 }
    pacemaker_failover_failed_leader = if ($IncludePacemakerFailoverGate) { $PacemakerFailoverFailedLeader } else { 0 }
    adapter_stability_runs = if ($IncludeAdapterStabilityGate) { $AdapterStabilityRuns } else { 0 }
    allowed_regression_pct = $AllowedRegressionPct
}

$summaryJson = Join-Path $OutputDir "acceptance-gate-summary.json"
$summaryMd = Join-Path $OutputDir "acceptance-gate-summary.md"
$summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryJson -Encoding UTF8

$md = @(
    "# Migration Acceptance Gate Summary"
    ""
    "- generated_at_utc: $($summary.generated_at_utc)"
    "- profile_name: $($summary.profile_name)"
    "- full_snapshot_profile: $($summary.full_snapshot_profile)"
    "- full_snapshot_profile_v2: $($summary.full_snapshot_profile_v2)"
    "- overall_pass: $($summary.overall_pass)"
    "- functional_pass: $($summary.functional_pass)"
    "- governance_chain_audit_root_parity_pass: $($summary.governance_chain_audit_root_parity_pass)"
    "- performance_gate_enabled: $($summary.performance_gate_enabled)"
    "- performance_pass: $($summary.performance_pass)"
    "- chain_query_rpc_gate_enabled: $($summary.chain_query_rpc_gate_enabled)"
    "- chain_query_rpc_pass: $($summary.chain_query_rpc_pass)"
    "- chain_query_rpc_expected_requests: $($summary.chain_query_rpc_expected_requests)"
    "- chain_query_rpc_bind: $($summary.chain_query_rpc_bind)"
    "- governance_rpc_gate_enabled: $($summary.governance_rpc_gate_enabled)"
    "- governance_rpc_pass: $($summary.governance_rpc_pass)"
    "- governance_rpc_audit_persist_pass: $($summary.governance_rpc_audit_persist_pass)"
    "- governance_rpc_signature_scheme_reject_pass: $($summary.governance_rpc_signature_scheme_reject_pass)"
    "- governance_rpc_vote_verifier_startup_pass: $($summary.governance_rpc_vote_verifier_startup_pass)"
    "- governance_rpc_vote_verifier_staged_reject_pass: $($summary.governance_rpc_vote_verifier_staged_reject_pass)"
    "- governance_rpc_vote_verifier_execute_pass: $($summary.governance_rpc_vote_verifier_execute_pass)"
    "- governance_rpc_chain_audit_pass: $($summary.governance_rpc_chain_audit_pass)"
    "- governance_rpc_chain_audit_persist_pass: $($summary.governance_rpc_chain_audit_persist_pass)"
    "- governance_rpc_chain_audit_restart_pass: $($summary.governance_rpc_chain_audit_restart_pass)"
    "- governance_rpc_chain_audit_execute_verifier_pass: $($summary.governance_rpc_chain_audit_execute_verifier_pass)"
    "- governance_rpc_chain_audit_persist_execute_verifier_pass: $($summary.governance_rpc_chain_audit_persist_execute_verifier_pass)"
    "- governance_rpc_chain_audit_restart_execute_verifier_pass: $($summary.governance_rpc_chain_audit_restart_execute_verifier_pass)"
    "- governance_rpc_chain_audit_execute_verifier_proof_pass: $($summary.governance_rpc_chain_audit_execute_verifier_proof_pass)"
    "- governance_rpc_policy_chain_audit_consistency_pass: $($summary.governance_rpc_policy_chain_audit_consistency_pass)"
    "- governance_rpc_chain_audit_root_pass: $($summary.governance_rpc_chain_audit_root_pass)"
    "- governance_rpc_chain_audit_persist_root_pass: $($summary.governance_rpc_chain_audit_persist_root_pass)"
    "- governance_rpc_chain_audit_restart_root_pass: $($summary.governance_rpc_chain_audit_restart_root_pass)"
    "- governance_rpc_chain_audit_root_proof_pass: $($summary.governance_rpc_chain_audit_root_proof_pass)"
    "- governance_rpc_mldsa_ffi_gate_enabled: $($summary.governance_rpc_mldsa_ffi_gate_enabled)"
    "- governance_rpc_mldsa_ffi_pass: $($summary.governance_rpc_mldsa_ffi_pass)"
    "- governance_rpc_mldsa_ffi_startup_pass: $($summary.governance_rpc_mldsa_ffi_startup_pass)"
    "- governance_rpc_expected_requests: $($summary.governance_rpc_expected_requests)"
    "- governance_rpc_bind: $($summary.governance_rpc_bind)"
    "- governance_rpc_mldsa_ffi_expected_requests: $($summary.governance_rpc_mldsa_ffi_expected_requests)"
    "- governance_rpc_mldsa_ffi_bind: $($summary.governance_rpc_mldsa_ffi_bind)"
    "- header_sync_gate_enabled: $($summary.header_sync_gate_enabled)"
    "- header_sync_pass: $($summary.header_sync_pass)"
    "- fast_state_sync_gate_enabled: $($summary.fast_state_sync_gate_enabled)"
    "- fast_state_sync_pass: $($summary.fast_state_sync_pass)"
    "- network_dos_gate_enabled: $($summary.network_dos_gate_enabled)"
    "- network_dos_pass: $($summary.network_dos_pass)"
    "- pacemaker_failover_gate_enabled: $($summary.pacemaker_failover_gate_enabled)"
    "- pacemaker_failover_pass: $($summary.pacemaker_failover_pass)"
    "- pacemaker_failover_nodes: $($summary.pacemaker_failover_nodes)"
    "- pacemaker_failover_failed_leader: $($summary.pacemaker_failover_failed_leader)"
    "- slash_governance_gate_enabled: $($summary.slash_governance_gate_enabled)"
    "- slash_governance_pass: $($summary.slash_governance_pass)"
    "- slash_policy_external_gate_enabled: $($summary.slash_policy_external_gate_enabled)"
    "- slash_policy_external_pass: $($summary.slash_policy_external_pass)"
    "- governance_hook_gate_enabled: $($summary.governance_hook_gate_enabled)"
    "- governance_hook_pass: $($summary.governance_hook_pass)"
    "- governance_execution_gate_enabled: $($summary.governance_execution_gate_enabled)"
    "- governance_execution_pass: $($summary.governance_execution_pass)"
    "- governance_param2_gate_enabled: $($summary.governance_param2_gate_enabled)"
    "- governance_param2_pass: $($summary.governance_param2_pass)"
    "- governance_param3_gate_enabled: $($summary.governance_param3_gate_enabled)"
    "- governance_param3_pass: $($summary.governance_param3_pass)"
    "- governance_market_policy_gate_enabled: $($summary.governance_market_policy_gate_enabled)"
    "- governance_market_policy_pass: $($summary.governance_market_policy_pass)"
    "- governance_market_policy_engine_pass: $($summary.governance_market_policy_engine_pass)"
    "- governance_market_policy_treasury_pass: $($summary.governance_market_policy_treasury_pass)"
    "- governance_market_policy_orchestration_pass: $($summary.governance_market_policy_orchestration_pass)"
    "- governance_market_policy_dividend_pass: $($summary.governance_market_policy_dividend_pass)"
    "- governance_market_policy_foreign_payment_pass: $($summary.governance_market_policy_foreign_payment_pass)"
    "- governance_council_policy_gate_enabled: $($summary.governance_council_policy_gate_enabled)"
    "- governance_council_policy_pass: $($summary.governance_council_policy_pass)"
    "- governance_negative_gate_enabled: $($summary.governance_negative_gate_enabled)"
    "- governance_negative_pass: $($summary.governance_negative_pass)"
    "- governance_access_policy_gate_enabled: $($summary.governance_access_policy_gate_enabled)"
    "- governance_access_policy_pass: $($summary.governance_access_policy_pass)"
    "- governance_token_economics_gate_enabled: $($summary.governance_token_economics_gate_enabled)"
    "- governance_token_economics_pass: $($summary.governance_token_economics_pass)"
    "- governance_treasury_spend_gate_enabled: $($summary.governance_treasury_spend_gate_enabled)"
    "- governance_treasury_spend_pass: $($summary.governance_treasury_spend_pass)"
    "- economic_infra_dedicated_gate_enabled: $($summary.economic_infra_dedicated_gate_enabled)"
    "- economic_infra_dedicated_pass: $($summary.economic_infra_dedicated_pass)"
    "- economic_infra_dedicated_token_system_pass: $($summary.economic_infra_dedicated_token_system_pass)"
    "- economic_infra_dedicated_amm_pass: $($summary.economic_infra_dedicated_amm_pass)"
    "- economic_infra_dedicated_nav_redemption_pass: $($summary.economic_infra_dedicated_nav_redemption_pass)"
    "- economic_infra_dedicated_cdp_pass: $($summary.economic_infra_dedicated_cdp_pass)"
    "- economic_infra_dedicated_bond_pass: $($summary.economic_infra_dedicated_bond_pass)"
    "- economic_infra_dedicated_treasury_pass: $($summary.economic_infra_dedicated_treasury_pass)"
    "- economic_infra_dedicated_governance_system_pass: $($summary.economic_infra_dedicated_governance_system_pass)"
    "- economic_infra_dedicated_dividend_pool_pass: $($summary.economic_infra_dedicated_dividend_pool_pass)"
    "- economic_infra_dedicated_foreign_payment_pass: $($summary.economic_infra_dedicated_foreign_payment_pass)"
    "- economic_service_surface_gate_enabled: $($summary.economic_service_surface_gate_enabled)"
    "- economic_service_surface_pass: $($summary.economic_service_surface_pass)"
    "- economic_service_surface_token_system_pass: $($summary.economic_service_surface_token_system_pass)"
    "- economic_service_surface_amm_pass: $($summary.economic_service_surface_amm_pass)"
    "- economic_service_surface_cdp_pass: $($summary.economic_service_surface_cdp_pass)"
    "- economic_service_surface_bond_pass: $($summary.economic_service_surface_bond_pass)"
    "- economic_service_surface_nav_redemption_pass: $($summary.economic_service_surface_nav_redemption_pass)"
    "- economic_service_surface_treasury_pass: $($summary.economic_service_surface_treasury_pass)"
    "- economic_service_surface_governance_system_pass: $($summary.economic_service_surface_governance_system_pass)"
    "- economic_service_surface_dividend_pool_pass: $($summary.economic_service_surface_dividend_pool_pass)"
    "- economic_service_surface_foreign_payment_pass: $($summary.economic_service_surface_foreign_payment_pass)"
    "- ops_control_surface_gate_enabled: $($summary.ops_control_surface_gate_enabled)"
    "- ops_control_surface_pass: $($summary.ops_control_surface_pass)"
    "- ops_control_surface_rate_limit_pass: $($summary.ops_control_surface_rate_limit_pass)"
    "- ops_control_surface_circuit_breaker_pass: $($summary.ops_control_surface_circuit_breaker_pass)"
    "- ops_control_surface_quota_pass: $($summary.ops_control_surface_quota_pass)"
    "- ops_control_surface_alert_field_pass: $($summary.ops_control_surface_alert_field_pass)"
    "- ops_control_surface_audit_field_pass: $($summary.ops_control_surface_audit_field_pass)"
    "- market_engine_treasury_negative_gate_enabled: $($summary.market_engine_treasury_negative_gate_enabled)"
    "- market_engine_treasury_negative_pass: $($summary.market_engine_treasury_negative_pass)"
    "- foreign_rate_source_gate_enabled: $($summary.foreign_rate_source_gate_enabled)"
    "- foreign_rate_source_pass: $($summary.foreign_rate_source_pass)"
    "- nav_valuation_source_gate_enabled: $($summary.nav_valuation_source_gate_enabled)"
    "- nav_valuation_source_pass: $($summary.nav_valuation_source_pass)"
    "- dividend_balance_source_gate_enabled: $($summary.dividend_balance_source_gate_enabled)"
    "- dividend_balance_source_pass: $($summary.dividend_balance_source_pass)"
    "- unified_account_gate_enabled: $($summary.unified_account_gate_enabled)"
    "- unified_account_pass: $($summary.unified_account_pass)"
    "- rpc_exposure_gate_enabled: $($summary.rpc_exposure_gate_enabled)"
    "- rpc_exposure_pass: $($summary.rpc_exposure_pass)"
    "- testnet_bootstrap_gate_enabled: $($summary.testnet_bootstrap_gate_enabled)"
    "- testnet_bootstrap_pass: $($summary.testnet_bootstrap_pass)"
    "- rpc_exposure_public_bind: $($summary.rpc_exposure_public_bind)"
    "- rpc_exposure_gov_bind: $($summary.rpc_exposure_gov_bind)"
    "- unjail_cooldown_gate_enabled: $($summary.unjail_cooldown_gate_enabled)"
    "- unjail_cooldown_pass: $($summary.unjail_cooldown_pass)"
    "- adapter_stability_enabled: $($summary.adapter_stability_enabled)"
    "- adapter_stability_pass: $($summary.adapter_stability_pass)"
    "- vm_runtime_split_gate_enabled: $($summary.vm_runtime_split_gate_enabled)"
    "- vm_runtime_split_pass: $($summary.vm_runtime_split_pass)"
    "- evm_chain_profile_signal_gate_enabled: $($summary.evm_chain_profile_signal_gate_enabled)"
    "- evm_chain_profile_signal_pass: $($summary.evm_chain_profile_signal_pass)"
    "- evm_tx_type_signal_gate_enabled: $($summary.evm_tx_type_signal_gate_enabled)"
    "- evm_tx_type_signal_pass: $($summary.evm_tx_type_signal_pass)"
    "- overlap_router_signal_gate_enabled: $($summary.overlap_router_signal_gate_enabled)"
    "- overlap_router_signal_pass: $($summary.overlap_router_signal_pass)"
    "- evm_backend_compare_gate_enabled: $($summary.evm_backend_compare_gate_enabled)"
    "- evm_backend_compare_include_polygon: $($summary.evm_backend_compare_include_polygon)"
    "- evm_backend_compare_include_bnb: $($summary.evm_backend_compare_include_bnb)"
    "- evm_backend_compare_include_avalanche: $($summary.evm_backend_compare_include_avalanche)"
    "- evm_backend_compare_evm_pass: $($summary.evm_backend_compare_evm_pass)"
    "- evm_backend_compare_polygon_pass: $($summary.evm_backend_compare_polygon_pass)"
    "- evm_backend_compare_bnb_pass: $($summary.evm_backend_compare_bnb_pass)"
    "- evm_backend_compare_avalanche_pass: $($summary.evm_backend_compare_avalanche_pass)"
    "- evm_backend_compare_pass: $($summary.evm_backend_compare_pass)"
    "- performance_runs: $($summary.performance_runs)"
    "- adapter_stability_runs: $($summary.adapter_stability_runs)"
    "- allowed_regression_pct: $($summary.allowed_regression_pct)"
    "- functional_report_json: $($summary.functional_report_json)"
    "- performance_report_json: $($summary.performance_report_json)"
    "- chain_query_rpc_report_json: $($summary.chain_query_rpc_report_json)"
    "- governance_rpc_report_json: $($summary.governance_rpc_report_json)"
    "- governance_rpc_mldsa_ffi_report_json: $($summary.governance_rpc_mldsa_ffi_report_json)"
    "- header_sync_report_json: $($summary.header_sync_report_json)"
    "- fast_state_sync_report_json: $($summary.fast_state_sync_report_json)"
    "- network_dos_report_json: $($summary.network_dos_report_json)"
    "- pacemaker_failover_report_json: $($summary.pacemaker_failover_report_json)"
    "- slash_governance_report_json: $($summary.slash_governance_report_json)"
    "- slash_policy_external_report_json: $($summary.slash_policy_external_report_json)"
    "- governance_hook_report_json: $($summary.governance_hook_report_json)"
    "- governance_execution_report_json: $($summary.governance_execution_report_json)"
    "- governance_param2_report_json: $($summary.governance_param2_report_json)"
    "- governance_param3_report_json: $($summary.governance_param3_report_json)"
    "- governance_market_policy_report_json: $($summary.governance_market_policy_report_json)"
    "- governance_council_policy_report_json: $($summary.governance_council_policy_report_json)"
    "- governance_negative_report_json: $($summary.governance_negative_report_json)"
    "- governance_access_policy_report_json: $($summary.governance_access_policy_report_json)"
    "- governance_token_economics_report_json: $($summary.governance_token_economics_report_json)"
    "- governance_treasury_spend_report_json: $($summary.governance_treasury_spend_report_json)"
    "- economic_infra_dedicated_report_json: $($summary.economic_infra_dedicated_report_json)"
    "- economic_service_surface_report_json: $($summary.economic_service_surface_report_json)"
    "- ops_control_surface_report_json: $($summary.ops_control_surface_report_json)"
    "- market_engine_treasury_negative_report_json: $($summary.market_engine_treasury_negative_report_json)"
    "- foreign_rate_source_report_json: $($summary.foreign_rate_source_report_json)"
    "- nav_valuation_source_report_json: $($summary.nav_valuation_source_report_json)"
    "- dividend_balance_source_report_json: $($summary.dividend_balance_source_report_json)"
    "- unified_account_report_json: $($summary.unified_account_report_json)"
    "- rpc_exposure_report_json: $($summary.rpc_exposure_report_json)"
    "- testnet_bootstrap_report_json: $($summary.testnet_bootstrap_report_json)"
    "- unjail_cooldown_report_json: $($summary.unjail_cooldown_report_json)"
    "- adapter_stability_report_json: $($summary.adapter_stability_report_json)"
    "- vm_runtime_split_report_json: $($summary.vm_runtime_split_report_json)"
    "- evm_chain_profile_signal_report_json: $($summary.evm_chain_profile_signal_report_json)"
    "- evm_tx_type_signal_report_json: $($summary.evm_tx_type_signal_report_json)"
    "- overlap_router_signal_report_json: $($summary.overlap_router_signal_report_json)"
    "- evm_backend_compare_evm_report_json: $($summary.evm_backend_compare_evm_report_json)"
    "- evm_backend_compare_polygon_report_json: $($summary.evm_backend_compare_polygon_report_json)"
    "- evm_backend_compare_bnb_report_json: $($summary.evm_backend_compare_bnb_report_json)"
    "- evm_backend_compare_avalanche_report_json: $($summary.evm_backend_compare_avalanche_report_json)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "acceptance gate summary:"
Write-Host "  profile_name: $profileName"
Write-Host "  full_snapshot_profile: $([bool]($FullSnapshotProfile -or $FullSnapshotProfileV2 -or $FullSnapshotProfileGA))"
Write-Host "  full_snapshot_profile_v2: $([bool]$FullSnapshotProfileV2)"
Write-Host "  overall_pass: $overallPass"
Write-Host "  functional_report: $functionalJson"
if ($IncludePerformanceGate) {
    Write-Host "  performance_report: $performanceJson"
} else {
    Write-Host "  performance_report: skipped"
}
if ($IncludeChainQueryRpcGate) {
    Write-Host "  chain_query_rpc_report: $chainQueryRpcJson"
}
if ($IncludeGovernanceRpcGate) {
    Write-Host "  governance_rpc_report: $governanceRpcJson"
}
if ($IncludeGovernanceRpcMldsaFfiGate) {
    Write-Host "  governance_rpc_mldsa_ffi_report: $governanceRpcMldsaFfiJson"
}
if ($IncludeHeaderSyncGate) {
    Write-Host "  header_sync_report: $headerSyncJson"
}
if ($IncludeFastStateSyncGate) {
    Write-Host "  fast_state_sync_report: $fastStateSyncJson"
}
if ($IncludeNetworkDosGate) {
    Write-Host "  network_dos_report: $networkDosJson"
}
if ($IncludePacemakerFailoverGate) {
    Write-Host "  pacemaker_failover_report: $pacemakerFailoverJson"
}
if ($IncludeSlashGovernanceGate) {
    Write-Host "  slash_governance_report: $slashGovernanceJson"
}
if ($IncludeSlashPolicyExternalGate) {
    Write-Host "  slash_policy_external_report: $slashPolicyExternalJson"
}
if ($IncludeGovernanceHookGate) {
    Write-Host "  governance_hook_report: $governanceHookJson"
}
if ($IncludeGovernanceExecutionGate) {
    Write-Host "  governance_execution_report: $governanceExecutionJson"
}
if ($IncludeGovernanceParam2Gate) {
    Write-Host "  governance_param2_report: $governanceParam2Json"
}
if ($IncludeGovernanceParam3Gate) {
    Write-Host "  governance_param3_report: $governanceParam3Json"
}
if ($IncludeGovernanceMarketPolicyGate) {
    Write-Host "  governance_market_policy_report: $governanceMarketPolicyJson"
}
if ($IncludeGovernanceCouncilPolicyGate) {
    Write-Host "  governance_council_policy_report: $governanceCouncilPolicyJson"
}
if ($IncludeGovernanceNegativeGate) {
    Write-Host "  governance_negative_report: $governanceNegativeJson"
}
if ($IncludeGovernanceAccessPolicyGate) {
    Write-Host "  governance_access_policy_report: $governanceAccessPolicyJson"
}
if ($IncludeGovernanceTokenEconomicsGate) {
    Write-Host "  governance_token_economics_report: $governanceTokenEconomicsJson"
}
if ($IncludeGovernanceTreasurySpendGate) {
    Write-Host "  governance_treasury_spend_report: $governanceTreasurySpendJson"
}
if ($IncludeEconomicInfraDedicatedGate) {
    Write-Host "  economic_infra_dedicated_report: $economicInfraDedicatedJson"
}
if ($IncludeEconomicServiceSurfaceGate) {
    Write-Host "  economic_service_surface_report: $economicServiceSurfaceJson"
}
if ($IncludeOpsControlSurfaceGate) {
    Write-Host "  ops_control_surface_report: $opsControlSurfaceJson"
}
if ($IncludeMarketEngineTreasuryNegativeGate) {
    Write-Host "  market_engine_treasury_negative_report: $marketEngineTreasuryNegativeJson"
}
if ($IncludeForeignRateSourceGate) {
    Write-Host "  foreign_rate_source_report: $foreignRateSourceJson"
}
if ($IncludeNavValuationSourceGate) {
    Write-Host "  nav_valuation_source_report: $navValuationSourceJson"
}
if ($IncludeDividendBalanceSourceGate) {
    Write-Host "  dividend_balance_source_report: $dividendBalanceSourceJson"
}
if ($IncludeUnifiedAccountGate) {
    Write-Host "  unified_account_report: $unifiedAccountJson"
}
if ($IncludeRpcExposureGate) {
    Write-Host "  rpc_exposure_report: $rpcExposureJson"
}
if ($IncludeTestnetBootstrapGate) {
    Write-Host "  testnet_bootstrap_report: $testnetBootstrapJson"
}
if ($IncludeUnjailCooldownGate) {
    Write-Host "  unjail_cooldown_report: $unjailCooldownJson"
}
if ($IncludeAdapterStabilityGate) {
    Write-Host "  adapter_stability_report: $adapterStabilityJson"
}
if ($IncludeVmRuntimeSplitGate) {
    Write-Host "  vm_runtime_split_report: $vmRuntimeSplitJson"
}
if ($IncludeEvmChainProfileSignalGate) {
    Write-Host "  evm_chain_profile_signal_report: $evmChainProfileSignalJson"
}
if ($IncludeEvmTxTypeSignalGate) {
    Write-Host "  evm_tx_type_signal_report: $evmTxTypeSignalJson"
}
if ($IncludeOverlapRouterSignalGate) {
    Write-Host "  overlap_router_signal_report: $overlapRouterSignalJson"
}
if ($IncludeEvmBackendCompareGate) {
    Write-Host "  evm_backend_compare_evm_report: $evmBackendCompareEvmJson"
    if ($EvmBackendCompareIncludePolygon) {
        Write-Host "  evm_backend_compare_polygon_report: $evmBackendComparePolygonJson"
    }
    if ($EvmBackendCompareIncludeBnb) {
        Write-Host "  evm_backend_compare_bnb_report: $evmBackendCompareBnbJson"
    }
    if ($EvmBackendCompareIncludeAvalanche) {
        Write-Host "  evm_backend_compare_avalanche_report: $evmBackendCompareAvalancheJson"
    }
}
Write-Host "  summary_json: $summaryJson"

if (-not $overallPass) {
    throw "migration acceptance gate FAILED (functional_pass=$functionalPass, governance_chain_audit_root_parity_pass=$governanceChainAuditRootParityPass, performance_pass=$performancePass, chain_query_rpc_pass=$chainQueryRpcPass, governance_rpc_pass=$governanceRpcPass, governance_rpc_audit_persist_pass=$governanceRpcAuditPersistPass, governance_rpc_signature_scheme_reject_pass=$governanceRpcSignatureSchemeRejectPass, governance_rpc_vote_verifier_startup_pass=$governanceRpcVoteVerifierStartupPass, governance_rpc_vote_verifier_staged_reject_pass=$governanceRpcVoteVerifierStagedRejectPass, governance_rpc_vote_verifier_execute_pass=$governanceRpcVoteVerifierExecutePass, governance_rpc_chain_audit_pass=$governanceRpcChainAuditPass, governance_rpc_chain_audit_persist_pass=$governanceRpcChainAuditPersistPass, governance_rpc_chain_audit_restart_pass=$governanceRpcChainAuditRestartPass, governance_rpc_chain_audit_execute_verifier_pass=$governanceRpcChainAuditExecuteVerifierPass, governance_rpc_chain_audit_persist_execute_verifier_pass=$governanceRpcChainAuditPersistExecuteVerifierPass, governance_rpc_chain_audit_restart_execute_verifier_pass=$governanceRpcChainAuditRestartExecuteVerifierPass, governance_rpc_chain_audit_execute_verifier_proof_pass=$governanceRpcChainAuditExecuteVerifierProofPass, governance_rpc_policy_chain_audit_consistency_pass=$governanceRpcPolicyChainAuditConsistencyPass, governance_rpc_chain_audit_root_pass=$governanceRpcChainAuditRootPass, governance_rpc_chain_audit_persist_root_pass=$governanceRpcChainAuditPersistRootPass, governance_rpc_chain_audit_restart_root_pass=$governanceRpcChainAuditRestartRootPass, governance_rpc_chain_audit_root_proof_pass=$governanceRpcChainAuditRootProofPass, governance_rpc_mldsa_ffi_pass=$governanceRpcMldsaFfiPass, governance_rpc_mldsa_ffi_startup_pass=$governanceRpcMldsaFfiStartupPass, header_sync_pass=$headerSyncPass, fast_state_sync_pass=$fastStateSyncPass, network_dos_pass=$networkDosPass, pacemaker_failover_pass=$pacemakerFailoverPass, slash_governance_pass=$slashGovernancePass, slash_policy_external_pass=$slashPolicyExternalPass, governance_hook_pass=$governanceHookPass, governance_execution_pass=$governanceExecutionPass, governance_param2_pass=$governanceParam2Pass, governance_param3_pass=$governanceParam3Pass, governance_market_policy_pass=$governanceMarketPolicyPass, governance_market_policy_engine_pass=$governanceMarketPolicyEnginePass, governance_market_policy_treasury_pass=$governanceMarketPolicyTreasuryPass, governance_market_policy_orchestration_pass=$governanceMarketPolicyOrchestrationPass, governance_market_policy_dividend_pass=$governanceMarketPolicyDividendPass, governance_market_policy_foreign_payment_pass=$governanceMarketPolicyForeignPass, governance_council_policy_pass=$governanceCouncilPolicyPass, governance_negative_pass=$governanceNegativePass, governance_access_policy_pass=$governanceAccessPolicyPass, governance_token_economics_pass=$governanceTokenEconomicsPass, governance_treasury_spend_pass=$governanceTreasurySpendPass, economic_infra_dedicated_pass=$economicInfraDedicatedPass, economic_infra_dedicated_token_system_pass=$economicInfraDedicatedTokenPass, economic_infra_dedicated_amm_pass=$economicInfraDedicatedAmmPass, economic_infra_dedicated_nav_redemption_pass=$economicInfraDedicatedNavPass, economic_infra_dedicated_cdp_pass=$economicInfraDedicatedCdpPass, economic_infra_dedicated_bond_pass=$economicInfraDedicatedBondPass, economic_infra_dedicated_treasury_pass=$economicInfraDedicatedTreasuryPass, economic_infra_dedicated_governance_system_pass=$economicInfraDedicatedGovernancePass, economic_infra_dedicated_dividend_pool_pass=$economicInfraDedicatedDividendPass, economic_infra_dedicated_foreign_payment_pass=$economicInfraDedicatedForeignPass, economic_service_surface_pass=$economicServiceSurfacePass, economic_service_surface_token_system_pass=$economicServiceSurfaceTokenPass, economic_service_surface_amm_pass=$economicServiceSurfaceAmmPass, economic_service_surface_cdp_pass=$economicServiceSurfaceCdpPass, economic_service_surface_bond_pass=$economicServiceSurfaceBondPass, economic_service_surface_nav_redemption_pass=$economicServiceSurfaceNavPass, economic_service_surface_treasury_pass=$economicServiceSurfaceTreasuryPass, economic_service_surface_governance_system_pass=$economicServiceSurfaceGovernancePass, economic_service_surface_dividend_pool_pass=$economicServiceSurfaceDividendPass, economic_service_surface_foreign_payment_pass=$economicServiceSurfaceForeignPass, ops_control_surface_pass=$opsControlSurfacePass, ops_control_surface_rate_limit_pass=$opsControlSurfaceRateLimitPass, ops_control_surface_circuit_breaker_pass=$opsControlSurfaceCircuitBreakerPass, ops_control_surface_quota_pass=$opsControlSurfaceQuotaPass, ops_control_surface_alert_field_pass=$opsControlSurfaceAlertFieldPass, ops_control_surface_audit_field_pass=$opsControlSurfaceAuditFieldPass, market_engine_treasury_negative_pass=$marketEngineTreasuryNegativePass, foreign_rate_source_pass=$foreignRateSourcePass, nav_valuation_source_pass=$navValuationSourcePass, dividend_balance_source_pass=$dividendBalanceSourcePass, unified_account_pass=$unifiedAccountPass, rpc_exposure_pass=$rpcExposurePass, testnet_bootstrap_pass=$testnetBootstrapPass, unjail_cooldown_pass=$unjailCooldownPass, adapter_stability_pass=$adapterStabilityPass, vm_runtime_split_pass=$vmRuntimeSplitPass, evm_chain_profile_signal_pass=$evmChainProfileSignalPass, evm_tx_type_signal_pass=$evmTxTypeSignalPass, overlap_router_signal_pass=$overlapRouterSignalPass, evm_backend_compare_pass=$evmBackendComparePass, evm_backend_compare_evm_pass=$evmBackendCompareEvmPass, evm_backend_compare_polygon_pass=$evmBackendComparePolygonPass, evm_backend_compare_bnb_pass=$evmBackendCompareBnbPass, evm_backend_compare_avalanche_pass=$evmBackendCompareAvalanchePass)"
}

Write-Host "migration acceptance gate PASS"
