[CmdletBinding()]
param(
    [string]$RepoRoot = "",
    [string]$ReconcileScript = "scripts/novovm-l1l4-reconcile.ps1",
    [ValidateRange(1, 86400)]
    [int]$IntervalSeconds = 15,
    [ValidateRange(1, 3600)]
    [int]$RestartDelaySeconds = 3,
    [ValidateRange(0, 1000000)]
    [int]$MaxCycles = 0,
    [ValidateRange(0, 1000000)]
    [int]$MaxFailures = 0,
    [string]$DispatchIndexFile = "artifacts/l1/l1l4-payout-dispatch.jsonl",
    [string]$SubmittedIndexFile = "artifacts/l1/l1l4-payout-submitted.jsonl",
    [string]$AddressMapFile = "artifacts/l1/payout-address-map.json",
    [string]$OutputDir = "artifacts/l1/payout-reconcile",
    [string]$ReconcileIndexFile = "artifacts/l1/l1l4-payout-reconcile.jsonl",
    [string]$StateFile = "artifacts/l1/l1l4-payout-state.json",
    [string]$CursorFile = "artifacts/l1/l1l4-payout-reconcile.cursor",
    [string]$RpcEndpoint = "http://127.0.0.1:9899",
    [string]$ConfirmMethod = "eth_getTransactionReceipt",
    [string]$SubmitMethod = "eth_sendTransaction",
    [string]$SenderAddress = "",
    [UInt64]$WeiPerRewardUnit = 1,
    [UInt64]$GasLimit = 21000,
    [UInt64]$MaxFeePerGasWei = 0,
    [UInt64]$MaxPriorityFeePerGasWei = 0,
    [int]$RpcTimeoutSec = 15,
    [ValidateRange(0, 1000)]
    [int]$ReplayMaxPerPayout = 3,
    [ValidateRange(0, 86400)]
    [int]$ReplayCooldownSec = 30,
    [switch]$FullReplayFirstCycle
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
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

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$reconcileScriptPath = Resolve-FullPath -Root $RepoRoot -Value $ReconcileScript
if (-not (Test-Path -LiteralPath $reconcileScriptPath)) {
    throw ("reconcile script not found: " + $reconcileScriptPath)
}

$cycle = 0
$failureCount = 0
$firstCycle = $true

while ($true) {
    $cycle = $cycle + 1
    Write-Host ("l1l4_reconcile_daemon_cycle_in: cycle={0} failures={1} interval_sec={2}" -f $cycle, $failureCount, $IntervalSeconds)

    $invokeArgs = @{
        RepoRoot = $RepoRoot
        DispatchIndexFile = $DispatchIndexFile
        SubmittedIndexFile = $SubmittedIndexFile
        AddressMapFile = $AddressMapFile
        OutputDir = $OutputDir
        ReconcileIndexFile = $ReconcileIndexFile
        StateFile = $StateFile
        CursorFile = $CursorFile
        RpcEndpoint = $RpcEndpoint
        ConfirmMethod = $ConfirmMethod
        SubmitMethod = $SubmitMethod
        SenderAddress = $SenderAddress
        WeiPerRewardUnit = $WeiPerRewardUnit
        GasLimit = $GasLimit
        MaxFeePerGasWei = $MaxFeePerGasWei
        MaxPriorityFeePerGasWei = $MaxPriorityFeePerGasWei
        RpcTimeoutSec = $RpcTimeoutSec
        ReplayMaxPerPayout = $ReplayMaxPerPayout
        ReplayCooldownSec = $ReplayCooldownSec
    }
    if ($firstCycle -and $FullReplayFirstCycle) {
        $invokeArgs["FullReplay"] = $true
    }

    $ok = $true
    try {
        & $reconcileScriptPath @invokeArgs
    } catch {
        $ok = $false
        $failureCount = $failureCount + 1
        Write-Host ("l1l4_reconcile_daemon_cycle_err: cycle={0} err={1}" -f $cycle, $_.Exception.Message)
    }

    if ($ok) {
        Write-Host ("l1l4_reconcile_daemon_cycle_out: cycle={0} ok=true" -f $cycle)
    }

    $firstCycle = $false

    if ($MaxCycles -gt 0 -and $cycle -ge $MaxCycles) {
        Write-Host ("l1l4_reconcile_daemon_out: reason=max_cycles cycle={0}" -f $cycle)
        break
    }
    if ($MaxFailures -gt 0 -and $failureCount -ge $MaxFailures) {
        Write-Host ("l1l4_reconcile_daemon_out: reason=max_failures failures={0}" -f $failureCount)
        break
    }

    if ($ok) {
        Start-Sleep -Seconds $IntervalSeconds
    } else {
        Start-Sleep -Seconds $RestartDelaySeconds
    }
}
