[CmdletBinding()]
param(
    [ValidateSet("prod", "dev")]
    [string]$Profile = "prod",
    [switch]$NoGateway,
    [switch]$BuildBeforeRun,
    [switch]$UseNodeWatchMode,
    [switch]$LeanIo,
    [ValidateRange(10, 60000)]
    [int]$PollMs = 200,
    [ValidateRange(50, 60000)]
    [int]$SupervisorPollMs = 1000,
    [ValidateRange(1, 1000000)]
    [int]$NodeWatchBatchMaxFiles = 1024,
    [ValidateRange(0, 86400)]
    [int]$IdleExitSeconds = 0,
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$SpoolDir = "artifacts/ingress/spool",
    [ValidateRange(0, 4294967295)]
    [uint32]$GatewayMaxRequests = 0,
    [ValidateRange(1, 3600)]
    [int]$RestartDelaySeconds = 3,
    [ValidateRange(0, 1000000)]
    [int]$MaxRestarts = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$upScript = Join-Path $repoRoot "scripts/novovm-up.ps1"
if (-not (Test-Path -LiteralPath $upScript)) {
    throw "missing entrypoint script: $upScript"
}

$args = @{
    Profile = $Profile
    Daemon = $true
    RestartDelaySeconds = $RestartDelaySeconds
    MaxRestarts = $MaxRestarts
    PollMs = $PollMs
    SupervisorPollMs = $SupervisorPollMs
    NodeWatchBatchMaxFiles = $NodeWatchBatchMaxFiles
    IdleExitSeconds = $IdleExitSeconds
    GatewayBind = $GatewayBind
    SpoolDir = $SpoolDir
    GatewayMaxRequests = $GatewayMaxRequests
}
if ($NoGateway) {
    $args["NoGateway"] = $true
}
if ($BuildBeforeRun) {
    $args["BuildBeforeRun"] = $true
}
if ($UseNodeWatchMode) {
    $args["UseNodeWatchMode"] = $true
}
if ($LeanIo) {
    $args["LeanIo"] = $true
}
& $upScript @args
