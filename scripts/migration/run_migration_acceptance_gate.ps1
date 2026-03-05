param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "",
    [double]$AllowedRegressionPct = -5.0,
    [ValidateRange(1, 9)]
    [int]$PerformanceRuns = 3,
    [switch]$FullSnapshotProfile,
    [switch]$FullSnapshotProfileV2,
    [bool]$IncludeChainQueryRpcGate = $true,
    [string]$ChainQueryRpcBind = "127.0.0.1:8899",
    [ValidateRange(1, 32)]
    [int]$ChainQueryRpcExpectedRequests = 5,
    [bool]$IncludeGovernanceRpcGate = $true,
    [string]$GovernanceRpcBind = "127.0.0.1:8901",
    [ValidateRange(1, 64)]
    [int]$GovernanceRpcExpectedRequests = 13,
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
    [bool]$IncludeGovernanceNegativeGate = $true,
    [bool]$IncludeRpcExposureGate = $false,
    [string]$RpcExposurePublicBind = "127.0.0.1:8899",
    [string]$RpcExposureGovBind = "127.0.0.1:8901",
    [bool]$IncludeUnjailCooldownGate = $true,
    [ValidateRange(4, 1000)]
    [int]$PacemakerFailoverNodes = 4,
    [ValidateRange(0, 999)]
    [int]$PacemakerFailoverFailedLeader = 0,
    [bool]$IncludeAdapterStabilityGate = $true,
    [ValidateRange(2, 20)]
    [int]$AdapterStabilityRuns = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$profileName = "default"
if ($FullSnapshotProfile -or $FullSnapshotProfileV2) {
    $IncludeChainQueryRpcGate = $true
    $IncludeGovernanceRpcGate = $true
    $IncludeHeaderSyncGate = $true
    $IncludeFastStateSyncGate = $true
    $IncludeNetworkDosGate = $true
    $IncludePacemakerFailoverGate = $true
    $IncludeSlashGovernanceGate = $true
    $IncludeSlashPolicyExternalGate = $true
    $IncludeGovernanceHookGate = $true
    $IncludeGovernanceExecutionGate = $true
    $IncludeGovernanceParam2Gate = $true
    $IncludeGovernanceParam3Gate = $true
    $IncludeGovernanceNegativeGate = $true
    $IncludeUnjailCooldownGate = $true
    $IncludeAdapterStabilityGate = $true
    $IncludeRpcExposureGate = $false
    $profileName = "full_snapshot_v1"
}
if ($FullSnapshotProfileV2) {
    $IncludeRpcExposureGate = $true
    $profileName = "full_snapshot_v2"
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
$governanceNegativeGateScript = Join-Path $RepoRoot "scripts\migration\run_governance_negative_gate.ps1"
$rpcExposureGateScript = Join-Path $RepoRoot "scripts\migration\run_rpc_exposure_gate.ps1"
$unjailCooldownGateScript = Join-Path $RepoRoot "scripts\migration\run_unjail_cooldown_gate.ps1"
$adapterStabilityScript = Join-Path $RepoRoot "scripts\migration\run_adapter_stability_gate.ps1"
Require-Path -Path $functionalScript -Name "functional script"
Require-Path -Path $performanceGateScript -Name "performance gate script"
if ($IncludeChainQueryRpcGate) {
    Require-Path -Path $chainQueryRpcGateScript -Name "chain query rpc gate script"
}
if ($IncludeGovernanceRpcGate) {
    Require-Path -Path $governanceRpcGateScript -Name "governance rpc gate script"
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
if ($IncludeGovernanceNegativeGate) {
    Require-Path -Path $governanceNegativeGateScript -Name "governance negative gate script"
}
if ($IncludeRpcExposureGate) {
    Require-Path -Path $rpcExposureGateScript -Name "rpc exposure gate script"
}
if ($IncludeUnjailCooldownGate) {
    Require-Path -Path $unjailCooldownGateScript -Name "unjail cooldown gate script"
}
if ($IncludeAdapterStabilityGate) {
    Require-Path -Path $adapterStabilityScript -Name "adapter stability gate script"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$functionalOutputDir = Join-Path $OutputDir "functional"
$performanceOutputDir = Join-Path $OutputDir "performance-gate"
$chainQueryRpcOutputDir = Join-Path $OutputDir "chain-query-rpc-gate"
$governanceRpcOutputDir = Join-Path $OutputDir "governance-rpc-gate"
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
$governanceNegativeOutputDir = Join-Path $OutputDir "governance-negative-gate"
$rpcExposureOutputDir = Join-Path $OutputDir "rpc-exposure-gate"
$unjailCooldownOutputDir = Join-Path $OutputDir "unjail-cooldown-gate"
$adapterStabilityOutputDir = Join-Path $OutputDir "adapter-stability-gate"
New-Item -ItemType Directory -Force -Path $functionalOutputDir | Out-Null
New-Item -ItemType Directory -Force -Path $performanceOutputDir | Out-Null
if ($IncludeChainQueryRpcGate) {
    New-Item -ItemType Directory -Force -Path $chainQueryRpcOutputDir | Out-Null
}
if ($IncludeGovernanceRpcGate) {
    New-Item -ItemType Directory -Force -Path $governanceRpcOutputDir | Out-Null
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
if ($IncludeGovernanceNegativeGate) {
    New-Item -ItemType Directory -Force -Path $governanceNegativeOutputDir | Out-Null
}
if ($IncludeRpcExposureGate) {
    New-Item -ItemType Directory -Force -Path $rpcExposureOutputDir | Out-Null
}
if ($IncludeUnjailCooldownGate) {
    New-Item -ItemType Directory -Force -Path $unjailCooldownOutputDir | Out-Null
}
if ($IncludeAdapterStabilityGate) {
    New-Item -ItemType Directory -Force -Path $adapterStabilityOutputDir | Out-Null
}

Write-Host "acceptance gate: functional consistency ..."
& $functionalScript -RepoRoot $RepoRoot -OutputDir $functionalOutputDir | Out-Null

Write-Host "acceptance gate: performance seal gate ..."
& $performanceGateScript `
    -RepoRoot $RepoRoot `
    -OutputDir $performanceOutputDir `
    -AllowedRegressionPct $AllowedRegressionPct `
    -Runs $PerformanceRuns | Out-Null

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
        -ExpectedRequests $GovernanceRpcExpectedRequests | Out-Null
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

if ($IncludeGovernanceNegativeGate) {
    Write-Host "acceptance gate: governance negative gate ..."
    & $governanceNegativeGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $governanceNegativeOutputDir | Out-Null
}

if ($IncludeRpcExposureGate) {
    Write-Host "acceptance gate: rpc exposure gate ..."
    & $rpcExposureGateScript `
        -RepoRoot $RepoRoot `
        -OutputDir $rpcExposureOutputDir `
        -PublicBind $RpcExposurePublicBind `
        -GovBind $RpcExposureGovBind | Out-Null
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

$functionalJson = Join-Path $functionalOutputDir "functional-consistency.json"
$performanceJson = Join-Path $performanceOutputDir "performance-gate-summary.json"
if ($IncludeChainQueryRpcGate) {
    $chainQueryRpcJson = Join-Path $chainQueryRpcOutputDir "chain-query-rpc-gate-summary.json"
}
if ($IncludeGovernanceRpcGate) {
    $governanceRpcJson = Join-Path $governanceRpcOutputDir "governance-rpc-gate-summary.json"
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
if ($IncludeGovernanceNegativeGate) {
    $governanceNegativeJson = Join-Path $governanceNegativeOutputDir "governance-negative-gate-summary.json"
}
if ($IncludeRpcExposureGate) {
    $rpcExposureJson = Join-Path $rpcExposureOutputDir "rpc-exposure-gate-summary.json"
}
if ($IncludeUnjailCooldownGate) {
    $unjailCooldownJson = Join-Path $unjailCooldownOutputDir "unjail-cooldown-gate-summary.json"
}
if ($IncludeAdapterStabilityGate) {
    $adapterStabilityJson = Join-Path $adapterStabilityOutputDir "adapter-stability-summary.json"
}
Require-Path -Path $functionalJson -Name "functional report json"
Require-Path -Path $performanceJson -Name "performance gate summary json"
if ($IncludeChainQueryRpcGate) {
    Require-Path -Path $chainQueryRpcJson -Name "chain query rpc gate summary json"
}
if ($IncludeGovernanceRpcGate) {
    Require-Path -Path $governanceRpcJson -Name "governance rpc gate summary json"
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
if ($IncludeGovernanceNegativeGate) {
    Require-Path -Path $governanceNegativeJson -Name "governance negative gate summary json"
}
if ($IncludeRpcExposureGate) {
    Require-Path -Path $rpcExposureJson -Name "rpc exposure gate summary json"
}
if ($IncludeUnjailCooldownGate) {
    Require-Path -Path $unjailCooldownJson -Name "unjail cooldown gate summary json"
}
if ($IncludeAdapterStabilityGate) {
    Require-Path -Path $adapterStabilityJson -Name "adapter stability summary json"
}

$functional = Get-Content -Path $functionalJson -Raw | ConvertFrom-Json
$performance = Get-Content -Path $performanceJson -Raw | ConvertFrom-Json
if ($IncludeChainQueryRpcGate) {
    $chainQueryRpc = Get-Content -Path $chainQueryRpcJson -Raw | ConvertFrom-Json
}
if ($IncludeGovernanceRpcGate) {
    $governanceRpc = Get-Content -Path $governanceRpcJson -Raw | ConvertFrom-Json
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
if ($IncludeGovernanceNegativeGate) {
    $governanceNegative = Get-Content -Path $governanceNegativeJson -Raw | ConvertFrom-Json
}
if ($IncludeRpcExposureGate) {
    $rpcExposure = Get-Content -Path $rpcExposureJson -Raw | ConvertFrom-Json
}
if ($IncludeUnjailCooldownGate) {
    $unjailCooldown = Get-Content -Path $unjailCooldownJson -Raw | ConvertFrom-Json
}
if ($IncludeAdapterStabilityGate) {
    $adapterStability = Get-Content -Path $adapterStabilityJson -Raw | ConvertFrom-Json
}

$functionalPass = [bool]$functional.overall_pass
$performancePass = [bool]$performance.pass
if ($IncludeChainQueryRpcGate) {
    $chainQueryRpcPass = [bool]$chainQueryRpc.pass
} else {
    $chainQueryRpcPass = $true
}
if ($IncludeGovernanceRpcGate) {
    $governanceRpcPass = [bool]$governanceRpc.pass
} else {
    $governanceRpcPass = $true
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
if ($IncludeGovernanceNegativeGate) {
    $governanceNegativePass = [bool]$governanceNegative.pass
} else {
    $governanceNegativePass = $true
}
if ($IncludeRpcExposureGate) {
    $rpcExposurePass = [bool]$rpcExposure.pass
} else {
    $rpcExposurePass = $true
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
$overallPass = ($functionalPass -and $performancePass -and $chainQueryRpcPass -and $governanceRpcPass -and $headerSyncPass -and $fastStateSyncPass -and $networkDosPass -and $pacemakerFailoverPass -and $slashGovernancePass -and $slashPolicyExternalPass -and $governanceHookPass -and $governanceExecutionPass -and $governanceParam2Pass -and $governanceParam3Pass -and $governanceNegativePass -and $rpcExposurePass -and $unjailCooldownPass -and $adapterStabilityPass)

$summary = [ordered]@{
    generated_at_utc = [DateTime]::UtcNow.ToString("o")
    profile_name = $profileName
    full_snapshot_profile = [bool]($FullSnapshotProfile -or $FullSnapshotProfileV2)
    full_snapshot_profile_v2 = [bool]$FullSnapshotProfileV2
    overall_pass = $overallPass
    functional_pass = $functionalPass
    performance_pass = $performancePass
    chain_query_rpc_gate_enabled = $IncludeChainQueryRpcGate
    chain_query_rpc_pass = $chainQueryRpcPass
    governance_rpc_gate_enabled = $IncludeGovernanceRpcGate
    governance_rpc_pass = $governanceRpcPass
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
    governance_negative_gate_enabled = $IncludeGovernanceNegativeGate
    governance_negative_pass = $governanceNegativePass
    rpc_exposure_gate_enabled = $IncludeRpcExposureGate
    rpc_exposure_pass = $rpcExposurePass
    unjail_cooldown_gate_enabled = $IncludeUnjailCooldownGate
    unjail_cooldown_pass = $unjailCooldownPass
    adapter_stability_enabled = $IncludeAdapterStabilityGate
    adapter_stability_pass = $adapterStabilityPass
    functional_report_json = $functionalJson
    performance_report_json = $performanceJson
    chain_query_rpc_report_json = if ($IncludeChainQueryRpcGate) { $chainQueryRpcJson } else { "" }
    governance_rpc_report_json = if ($IncludeGovernanceRpcGate) { $governanceRpcJson } else { "" }
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
    governance_negative_report_json = if ($IncludeGovernanceNegativeGate) { $governanceNegativeJson } else { "" }
    rpc_exposure_report_json = if ($IncludeRpcExposureGate) { $rpcExposureJson } else { "" }
    unjail_cooldown_report_json = if ($IncludeUnjailCooldownGate) { $unjailCooldownJson } else { "" }
    adapter_stability_report_json = if ($IncludeAdapterStabilityGate) { $adapterStabilityJson } else { "" }
    performance_runs = $PerformanceRuns
    chain_query_rpc_expected_requests = if ($IncludeChainQueryRpcGate) { $ChainQueryRpcExpectedRequests } else { 0 }
    chain_query_rpc_bind = if ($IncludeChainQueryRpcGate) { $ChainQueryRpcBind } else { "" }
    governance_rpc_expected_requests = if ($IncludeGovernanceRpcGate) { $GovernanceRpcExpectedRequests } else { 0 }
    governance_rpc_bind = if ($IncludeGovernanceRpcGate) { $GovernanceRpcBind } else { "" }
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
    "- performance_pass: $($summary.performance_pass)"
    "- chain_query_rpc_gate_enabled: $($summary.chain_query_rpc_gate_enabled)"
    "- chain_query_rpc_pass: $($summary.chain_query_rpc_pass)"
    "- chain_query_rpc_expected_requests: $($summary.chain_query_rpc_expected_requests)"
    "- chain_query_rpc_bind: $($summary.chain_query_rpc_bind)"
    "- governance_rpc_gate_enabled: $($summary.governance_rpc_gate_enabled)"
    "- governance_rpc_pass: $($summary.governance_rpc_pass)"
    "- governance_rpc_expected_requests: $($summary.governance_rpc_expected_requests)"
    "- governance_rpc_bind: $($summary.governance_rpc_bind)"
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
    "- governance_negative_gate_enabled: $($summary.governance_negative_gate_enabled)"
    "- governance_negative_pass: $($summary.governance_negative_pass)"
    "- rpc_exposure_gate_enabled: $($summary.rpc_exposure_gate_enabled)"
    "- rpc_exposure_pass: $($summary.rpc_exposure_pass)"
    "- rpc_exposure_public_bind: $($summary.rpc_exposure_public_bind)"
    "- rpc_exposure_gov_bind: $($summary.rpc_exposure_gov_bind)"
    "- unjail_cooldown_gate_enabled: $($summary.unjail_cooldown_gate_enabled)"
    "- unjail_cooldown_pass: $($summary.unjail_cooldown_pass)"
    "- adapter_stability_enabled: $($summary.adapter_stability_enabled)"
    "- adapter_stability_pass: $($summary.adapter_stability_pass)"
    "- performance_runs: $($summary.performance_runs)"
    "- adapter_stability_runs: $($summary.adapter_stability_runs)"
    "- allowed_regression_pct: $($summary.allowed_regression_pct)"
    "- functional_report_json: $($summary.functional_report_json)"
    "- performance_report_json: $($summary.performance_report_json)"
    "- chain_query_rpc_report_json: $($summary.chain_query_rpc_report_json)"
    "- governance_rpc_report_json: $($summary.governance_rpc_report_json)"
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
    "- governance_negative_report_json: $($summary.governance_negative_report_json)"
    "- rpc_exposure_report_json: $($summary.rpc_exposure_report_json)"
    "- unjail_cooldown_report_json: $($summary.unjail_cooldown_report_json)"
    "- adapter_stability_report_json: $($summary.adapter_stability_report_json)"
)
$md -join "`n" | Set-Content -Path $summaryMd -Encoding UTF8

Write-Host "acceptance gate summary:"
Write-Host "  profile_name: $profileName"
Write-Host "  full_snapshot_profile: $([bool]($FullSnapshotProfile -or $FullSnapshotProfileV2))"
Write-Host "  full_snapshot_profile_v2: $([bool]$FullSnapshotProfileV2)"
Write-Host "  overall_pass: $overallPass"
Write-Host "  functional_report: $functionalJson"
Write-Host "  performance_report: $performanceJson"
if ($IncludeChainQueryRpcGate) {
    Write-Host "  chain_query_rpc_report: $chainQueryRpcJson"
}
if ($IncludeGovernanceRpcGate) {
    Write-Host "  governance_rpc_report: $governanceRpcJson"
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
if ($IncludeGovernanceNegativeGate) {
    Write-Host "  governance_negative_report: $governanceNegativeJson"
}
if ($IncludeRpcExposureGate) {
    Write-Host "  rpc_exposure_report: $rpcExposureJson"
}
if ($IncludeUnjailCooldownGate) {
    Write-Host "  unjail_cooldown_report: $unjailCooldownJson"
}
if ($IncludeAdapterStabilityGate) {
    Write-Host "  adapter_stability_report: $adapterStabilityJson"
}
Write-Host "  summary_json: $summaryJson"

if (-not $overallPass) {
    throw "migration acceptance gate FAILED (functional_pass=$functionalPass, performance_pass=$performancePass, chain_query_rpc_pass=$chainQueryRpcPass, governance_rpc_pass=$governanceRpcPass, header_sync_pass=$headerSyncPass, fast_state_sync_pass=$fastStateSyncPass, network_dos_pass=$networkDosPass, pacemaker_failover_pass=$pacemakerFailoverPass, slash_governance_pass=$slashGovernancePass, slash_policy_external_pass=$slashPolicyExternalPass, governance_hook_pass=$governanceHookPass, governance_execution_pass=$governanceExecutionPass, governance_param2_pass=$governanceParam2Pass, governance_param3_pass=$governanceParam3Pass, governance_negative_pass=$governanceNegativePass, rpc_exposure_pass=$rpcExposurePass, unjail_cooldown_pass=$unjailCooldownPass, adapter_stability_pass=$adapterStabilityPass)"
}

Write-Host "migration acceptance gate PASS"
