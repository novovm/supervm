[CmdletBinding()]
param(
    [ValidateSet("dev", "prod")]
    [string]$Profile = "dev",
    [ValidateSet("full", "l1", "l2", "l3")]
    [string]$RoleProfile = "full",
    [switch]$NoGateway,
    [switch]$SkipBuild,
    [switch]$BuildBeforeRun,
    [switch]$Daemon,
    [switch]$UseNodeWatchMode,
    [switch]$LeanIo,
    [ValidateRange(1, 3600)]
    [int]$RestartDelaySeconds = 3,
    [ValidateRange(0, 1000000)]
    [int]$MaxRestarts = 0,
    [ValidateSet("none", "backup", "restore", "migrate")]
    [string]$UaStoreAction = "none",
    [string]$UaSnapshot = "",
    [string]$GatewayStoreFrom = "",
    [string]$PluginStoreFrom = "",
    [string]$PluginAuditFrom = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$SpoolDir = "artifacts/ingress/spool",
    [ValidateRange(10, 60000)]
    [int]$PollMs = 200,
    [ValidateRange(50, 60000)]
    [int]$SupervisorPollMs = 1000,
    [ValidateRange(1, 1000000)]
    [int]$NodeWatchBatchMaxFiles = 1024,
    [switch]$EnableReconcileDaemon,
    [string]$ReconcileSenderAddress = "",
    [string]$ReconcileRpcEndpoint = "http://127.0.0.1:9899",
    [ValidateRange(1, 86400)]
    [int]$ReconcileIntervalSeconds = 15,
    [ValidateRange(1, 3600)]
    [int]$ReconcileRestartDelaySeconds = 3,
    [ValidateRange(0, 1000)]
    [int]$ReconcileReplayMaxPerPayout = 3,
    [ValidateRange(0, 86400)]
    [int]$ReconcileReplayCooldownSec = 30,
    [ValidateRange(0, 86400)]
    [int]$IdleExitSeconds = 0,
    [ValidateRange(0, 4294967295)]
    [uint32]$GatewayMaxRequests = 0,
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Set-EnvIfEmpty {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$Value
    )
    $item = Get-Item -Path ("Env:" + $Name) -ErrorAction SilentlyContinue
    if ($null -eq $item -or [string]::IsNullOrWhiteSpace($item.Value)) {
        Set-Item -Path ("Env:" + $Name) -Value $Value
    }
}

function Set-EnvForce {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$Value
    )
    Set-Item -Path ("Env:" + $Name) -Value $Value
}

function Invoke-UaStoreAction {
    param(
        [string]$RepoRootPath,
        [string]$Action,
        [string]$Snapshot,
        [string]$GatewayFrom,
        [string]$PluginFrom,
        [string]$AuditFrom,
        [switch]$BypassForce
    )
    $uaScript = Join-Path $RepoRootPath "scripts/novovm-ua-prod-store.ps1"
    if (-not (Test-Path -LiteralPath $uaScript)) {
        throw ("Missing UA store script: " + $uaScript)
    }
    $args = @{
        Action = $Action
        RepoRoot = $RepoRootPath
    }
    if ($Snapshot) {
        $args["Snapshot"] = $Snapshot
    }
    if ($GatewayFrom) {
        $args["GatewayStoreFrom"] = $GatewayFrom
    }
    if ($PluginFrom) {
        $args["PluginStoreFrom"] = $PluginFrom
    }
    if ($AuditFrom) {
        $args["PluginAuditFrom"] = $AuditFrom
    }
    if ($BypassForce) {
        $args["Force"] = $true
    }
    & $uaScript @args
}

function Invoke-PipelineOnce {
    param(
        [string]$PipelineScriptPath,
        [switch]$NoGatewayMode,
        [switch]$SkipBuildMode,
        [switch]$UseNodeWatchModeFlag,
        [switch]$LeanIoFlag,
        [switch]$EnableReconcileDaemonFlag,
        [string]$ReconcileSenderAddressValue,
        [string]$ReconcileRpcEndpointValue,
        [int]$ReconcileIntervalSecondsValue,
        [int]$ReconcileRestartDelaySecondsValue,
        [int]$ReconcileReplayMaxPerPayoutValue,
        [int]$ReconcileReplayCooldownSecValue,
        [string]$BindValue,
        [string]$SpoolDirValue,
        [int]$PollMsValue,
        [int]$SupervisorPollMsValue,
        [int]$NodeWatchBatchMaxFilesValue,
        [int]$IdleExitSecondsValue,
        [uint32]$GatewayMaxRequestsValue
    )
    $invokeArgs = @{}
    if ($NoGatewayMode) {
        $invokeArgs["SkipGatewayStart"] = $true
    }
    if ($SkipBuildMode) {
        $invokeArgs["SkipBuild"] = $true
    }
    if ($UseNodeWatchModeFlag) {
        $invokeArgs["UseNodeWatchMode"] = $true
    }
    if ($LeanIoFlag) {
        $invokeArgs["LeanIo"] = $true
    }
    if ($EnableReconcileDaemonFlag) {
        $invokeArgs["EnableReconcileDaemon"] = $true
        $invokeArgs["ReconcileSenderAddress"] = $ReconcileSenderAddressValue
        $invokeArgs["ReconcileRpcEndpoint"] = $ReconcileRpcEndpointValue
        $invokeArgs["ReconcileIntervalSeconds"] = $ReconcileIntervalSecondsValue
        $invokeArgs["ReconcileRestartDelaySeconds"] = $ReconcileRestartDelaySecondsValue
        $invokeArgs["ReconcileReplayMaxPerPayout"] = $ReconcileReplayMaxPerPayoutValue
        $invokeArgs["ReconcileReplayCooldownSec"] = $ReconcileReplayCooldownSecValue
    }
    $invokeArgs["GatewayBind"] = $BindValue
    $invokeArgs["SpoolDir"] = $SpoolDirValue
    $invokeArgs["PollMs"] = $PollMsValue
    $invokeArgs["SupervisorPollMs"] = $SupervisorPollMsValue
    $invokeArgs["NodeWatchBatchMaxFiles"] = $NodeWatchBatchMaxFilesValue
    $invokeArgs["IdleExitSeconds"] = $IdleExitSecondsValue
    $invokeArgs["GatewayMaxRequests"] = $GatewayMaxRequestsValue
    & $PipelineScriptPath @invokeArgs
}

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

if ($UaStoreAction -ne "none") {
    Invoke-UaStoreAction -RepoRootPath $repoRoot -Action $UaStoreAction -Snapshot $UaSnapshot -GatewayFrom $GatewayStoreFrom -PluginFrom $PluginStoreFrom -AuditFrom $PluginAuditFrom -BypassForce:$Force
    exit 0
}

Set-EnvIfEmpty -Name "NOVOVM_NODE_MODE" -Value "full"
Set-EnvIfEmpty -Name "NOVOVM_EXEC_PATH" -Value "ffi_v2"
Set-EnvIfEmpty -Name "NOVOVM_HOST_ADMISSION" -Value "disabled"
if ($env:COMPUTERNAME) {
    Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_NODE_ID" -Value $env:COMPUTERNAME
} elseif ($env:HOSTNAME) {
    Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_NODE_ID" -Value $env:HOSTNAME
} else {
    Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_NODE_ID" -Value "local"
}
Set-EnvIfEmpty -Name "NOVOVM_OVERLAY_SESSION_ID" -Value ("sess-" + [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
Set-EnvIfEmpty -Name "NOVOVM_L1L4_ANCHOR_LEDGER_ENABLED" -Value "0"
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_SPOOL_DIR" -Value $SpoolDir
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_UA_STORE_BACKEND" -Value "rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_UA_STORE_PATH" -Value "artifacts/gateway/unified-account-router.rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND" -Value "rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_GATEWAY_ETH_TX_INDEX_PATH" -Value "artifacts/gateway/eth-tx-index.rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND" -Value "rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH" -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND" -Value "rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH" -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.rocksdb"
Set-EnvIfEmpty -Name "NOVOVM_ALLOW_NON_PROD_PLUGIN_BACKEND" -Value "0"

if ($Profile -eq "prod") {
    Set-EnvForce -Name "NOVOVM_NODE_MODE" -Value "full"
    Set-EnvForce -Name "NOVOVM_EXEC_PATH" -Value "ffi_v2"
    Set-EnvForce -Name "NOVOVM_HOST_ADMISSION" -Value "disabled"
    Set-EnvForce -Name "NOVOVM_L1L4_ANCHOR_PATH" -Value "artifacts/l1/l1l4-anchor.jsonl"
    Set-EnvForce -Name "NOVOVM_L1L4_ANCHOR_LEDGER_ENABLED" -Value "1"
    Set-EnvIfEmpty -Name "NOVOVM_L1L4_ANCHOR_LEDGER_KEY_PREFIX" -Value "ledger:l1:l1l4_anchor:v1:"
    Set-EnvForce -Name "NOVOVM_GATEWAY_UA_STORE_BACKEND" -Value "rocksdb"
    Set-EnvForce -Name "NOVOVM_GATEWAY_UA_STORE_PATH" -Value "artifacts/gateway/unified-account-router.rocksdb"
    Set-EnvForce -Name "NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND" -Value "rocksdb"
    Set-EnvForce -Name "NOVOVM_GATEWAY_ETH_TX_INDEX_PATH" -Value "artifacts/gateway/eth-tx-index.rocksdb"
    Set-EnvForce -Name "NOVOVM_ADAPTER_PLUGIN_UA_STORE_BACKEND" -Value "rocksdb"
    Set-EnvForce -Name "NOVOVM_ADAPTER_PLUGIN_UA_STORE_PATH" -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-router.rocksdb"
    Set-EnvForce -Name "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_BACKEND" -Value "rocksdb"
    Set-EnvForce -Name "NOVOVM_ADAPTER_PLUGIN_UA_AUDIT_PATH" -Value "artifacts/migration/unifiedaccount/ua-plugin-self-guard-audit.rocksdb"
    Set-EnvForce -Name "NOVOVM_ALLOW_NON_PROD_UA_BACKEND" -Value "0"
    Set-EnvForce -Name "NOVOVM_ALLOW_NON_PROD_GATEWAY_BACKEND" -Value "0"
    Set-EnvForce -Name "NOVOVM_ALLOW_NON_PROD_PLUGIN_BACKEND" -Value "0"
    Set-EnvIfEmpty -Name "RUST_LOG" -Value "info"
} else {
    Set-EnvIfEmpty -Name "RUST_LOG" -Value "debug"
}

$pipelineScript = Join-Path $repoRoot "scripts/migration/run_gateway_node_pipeline.ps1"
if (-not (Test-Path -LiteralPath $pipelineScript)) {
    throw ("Missing pipeline script: " + $pipelineScript)
}

$effectiveSkipBuild = $SkipBuild -or (($Profile -eq "prod") -and (-not $BuildBeforeRun))
$effectiveNoGateway = $NoGateway
$effectiveUseNodeWatchMode = $UseNodeWatchMode -or (($Profile -eq "prod") -and $Daemon)
$effectiveLeanIo = $LeanIo -or (($Profile -eq "prod") -and $Daemon)
$effectiveEnableReconcileDaemon = $EnableReconcileDaemon -or (($Profile -eq "prod") -and $Daemon -and (-not [string]::IsNullOrWhiteSpace($ReconcileSenderAddress)))

switch ($RoleProfile) {
    "l1" {
        $effectiveNoGateway = $true
        $effectiveUseNodeWatchMode = $true
        $effectiveEnableReconcileDaemon = $false
        if ($Profile -eq "prod") {
            $effectiveLeanIo = $true
        }
    }
    "l2" {
        $effectiveNoGateway = $true
        $effectiveUseNodeWatchMode = $true
        $effectiveEnableReconcileDaemon = $false
        if ($Profile -eq "prod") {
            $effectiveLeanIo = $true
        }
    }
    "l3" {
        $effectiveUseNodeWatchMode = $true
        if ($Profile -eq "prod") {
            $effectiveLeanIo = $true
        }
    }
    default {
    }
}

Set-EnvForce -Name "NOVOVM_NODE_ROLE_PROFILE" -Value $RoleProfile

switch ($RoleProfile) {
    "l1" { Set-EnvForce -Name "NOVOVM_NETWORK_LAYER_HINT" -Value "L1" }
    "l2" { Set-EnvForce -Name "NOVOVM_NETWORK_LAYER_HINT" -Value "L2" }
    "l3" { Set-EnvForce -Name "NOVOVM_NETWORK_LAYER_HINT" -Value "L3" }
    default { Set-EnvForce -Name "NOVOVM_NETWORK_LAYER_HINT" -Value "L1-L4" }
}

if ($effectiveEnableReconcileDaemon -and $effectiveNoGateway) {
    throw "EnableReconcileDaemon requires local gateway process in unified entrypoint mode (do not use -NoGateway)"
}

Write-Host ("novovm_up_profile: profile={0} role={1} no_gateway={2} daemon={3} use_node_watch_mode={4} lean_io={5}" -f $Profile, $RoleProfile, [bool]$effectiveNoGateway, [bool]$Daemon, [bool]$effectiveUseNodeWatchMode, [bool]$effectiveLeanIo)
Write-Host ("novovm_up_reconcile_embedded: enabled={0} sender={1} endpoint={2} interval_sec={3} replay_max={4} replay_cooldown_sec={5}" -f [bool]$effectiveEnableReconcileDaemon, $ReconcileSenderAddress, $ReconcileRpcEndpoint, $ReconcileIntervalSeconds, $ReconcileReplayMaxPerPayout, $ReconcileReplayCooldownSec)

if (-not $Daemon) {
    Invoke-PipelineOnce -PipelineScriptPath $pipelineScript -NoGatewayMode:$effectiveNoGateway -SkipBuildMode:$effectiveSkipBuild -UseNodeWatchModeFlag:$effectiveUseNodeWatchMode -LeanIoFlag:$effectiveLeanIo -EnableReconcileDaemonFlag:$effectiveEnableReconcileDaemon -ReconcileSenderAddressValue $ReconcileSenderAddress -ReconcileRpcEndpointValue $ReconcileRpcEndpoint -ReconcileIntervalSecondsValue $ReconcileIntervalSeconds -ReconcileRestartDelaySecondsValue $ReconcileRestartDelaySeconds -ReconcileReplayMaxPerPayoutValue $ReconcileReplayMaxPerPayout -ReconcileReplayCooldownSecValue $ReconcileReplayCooldownSec -BindValue $GatewayBind -SpoolDirValue $SpoolDir -PollMsValue $PollMs -SupervisorPollMsValue $SupervisorPollMs -NodeWatchBatchMaxFilesValue $NodeWatchBatchMaxFiles -IdleExitSecondsValue $IdleExitSeconds -GatewayMaxRequestsValue $GatewayMaxRequests
    exit 0
}

$restartCount = 0
while ($true) {
    Write-Host ("novovm_up_daemon_cycle_in: profile={0} role={1} no_gateway={2} skip_build={3} use_node_watch_mode={4} lean_io={5} restart_count={6}" -f $Profile, $RoleProfile, [bool]$effectiveNoGateway, [bool]$effectiveSkipBuild, [bool]$effectiveUseNodeWatchMode, [bool]$effectiveLeanIo, $restartCount)
    $ok = $true
    try {
        Invoke-PipelineOnce -PipelineScriptPath $pipelineScript -NoGatewayMode:$effectiveNoGateway -SkipBuildMode:$effectiveSkipBuild -UseNodeWatchModeFlag:$effectiveUseNodeWatchMode -LeanIoFlag:$effectiveLeanIo -EnableReconcileDaemonFlag:$effectiveEnableReconcileDaemon -ReconcileSenderAddressValue $ReconcileSenderAddress -ReconcileRpcEndpointValue $ReconcileRpcEndpoint -ReconcileIntervalSecondsValue $ReconcileIntervalSeconds -ReconcileRestartDelaySecondsValue $ReconcileRestartDelaySeconds -ReconcileReplayMaxPerPayoutValue $ReconcileReplayMaxPerPayout -ReconcileReplayCooldownSecValue $ReconcileReplayCooldownSec -BindValue $GatewayBind -SpoolDirValue $SpoolDir -PollMsValue $PollMs -SupervisorPollMsValue $SupervisorPollMs -NodeWatchBatchMaxFilesValue $NodeWatchBatchMaxFiles -IdleExitSecondsValue $IdleExitSeconds -GatewayMaxRequestsValue $GatewayMaxRequests
    } catch {
        $ok = $false
        Write-Host ("novovm_up_daemon_cycle_err: {0}" -f $_.Exception.Message)
    }

    if ($ok) {
        Write-Host "novovm_up_daemon_cycle_out: pipeline exited without exception"
    }

    $restartCount = $restartCount + 1
    if ($MaxRestarts -gt 0 -and $restartCount -ge $MaxRestarts) {
        Write-Host ("novovm_up_daemon_out: max_restarts_reached={0}" -f $MaxRestarts)
        break
    }
    Start-Sleep -Seconds $RestartDelaySeconds
}
