<#
DEPRECATED COMPATIBILITY SHELL

This script is NOT part of the NOVOVM mainline entry path.
Mainline entry is `novovmctl`.

This script exists only for compatibility forwarding.

Do NOT add:
- policy logic
- rollout logic
- lifecycle logic
- daemon logic
- any business logic

Only parameter forwarding and exit-code passthrough are allowed.
#>
[CmdletBinding()]
param(
    [string]$Action = "status",
    [string]$CtlBinaryFile = "",
    [string]$RepoRoot = "",
    [string]$Version = "",
    [string]$TargetVersion = "",
    [string]$RollbackVersion = "",
    [string]$GatewayBinaryFrom = "",
    [string]$NodeBinaryFrom = "",
    [switch]$SetCurrent,
    [string]$ReleaseRoot = "artifacts/runtime/releases",
    [string]$RuntimeStateFile = "artifacts/runtime/lifecycle/state.json",
    [string]$AuditFile = "",
    [string]$RuntimePidFile = "artifacts/runtime/lifecycle/novovm-up.pid",
    [string]$RuntimeLogDir = "artifacts/runtime/lifecycle/logs",
    [string]$Profile = "prod",
    [string]$RoleProfile = "full",
    [switch]$NoGateway,
    [switch]$UseNodeWatchMode,
    [switch]$LeanIo,
    [switch]$EnableReconcileDaemon,
    [string]$ReconcileSenderAddress = "",
    [string]$ReconcileRpcEndpoint = "http://127.0.0.1:9899",
    [int]$ReconcileIntervalSeconds = 15,
    [int]$ReconcileRestartDelaySeconds = 3,
    [int]$ReconcileReplayMaxPerPayout = 3,
    [int]$ReconcileReplayCooldownSec = 30,
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$SpoolDir = "artifacts/ingress/spool",
    [int]$PollMs = 200,
    [int]$SupervisorPollMs = 1000,
    [int]$NodeWatchBatchMaxFiles = 1024,
    [int]$IdleExitSeconds = 0,
    [uint32]$GatewayMaxRequests = 0,
    [string]$OverlayRouteMode = "",
    [string]$OverlayRouteRuntimeFile = "config/runtime/lifecycle/overlay.route.runtime.json",
    [string]$OverlayRouteRuntimeProfile = "",
    [string]$OverlayRouteRelayDirectoryFile = "",
    [double]$OverlayRouteRelayHealthMin = 0,
    [string]$OverlayRouteRelayPenaltyStateFile = "artifacts/runtime/lifecycle/overlay.relay.penalty.state.json",
    [string]$OverlayRouteRelayPenaltyDelta = "",
    [double]$OverlayRouteRelayPenaltyRecoverPerRun = 0.05,
    [string]$OverlayRouteRelayCandidates = "",
    [string]$OverlayRouteRelayCandidatesByRegion = "",
    [string]$OverlayRouteRelayCandidatesByRole = "",
    [switch]$EnableAutoProfile,
    [string]$AutoProfileStateFile = "artifacts/runtime/lifecycle/overlay.auto-profile.state.json",
    [string]$AutoProfileProfiles = "prod-cn,prod-eu,prod-us",
    [int]$AutoProfileMinHoldSeconds = 180,
    [double]$AutoProfileSwitchMargin = 0.08,
    [int]$AutoProfileSwitchbackCooldownSeconds = 300,
    [int]$AutoProfileRecheckSeconds = 30,
    [string]$AutoProfileBinaryPath = "",
    [int]$StartGraceSeconds = 6,
    [int]$UpgradeHealthSeconds = 12,
    [string]$RuntimeTemplateFile = "",
    [switch]$RestartAfterSetRuntime,
    [string]$NodeGroup = "",
    [string]$UpgradeWindow = "",
    [string]$RequireNodeGroup = "",
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# COMPATIBILITY SHELL ONLY
# Mainline execution must go through `novovmctl lifecycle`.
# PowerShell here is limited to parameter forwarding and exit-code passthrough.

$script:NovovmCtlLifecycleSupportedParams = @(
    "Action","CtlBinaryFile","RepoRoot","Version","TargetVersion","RollbackVersion","GatewayBinaryFrom","NodeBinaryFrom",
    "SetCurrent","ReleaseRoot","RuntimeStateFile","AuditFile","RuntimePidFile","RuntimeLogDir",
    "Profile","RoleProfile","UseNodeWatchMode","PollMs","NodeWatchBatchMaxFiles","IdleExitSeconds",
    "OverlayRouteMode","OverlayRouteRuntimeFile","OverlayRouteRuntimeProfile","OverlayRouteRelayDirectoryFile",
    "EnableAutoProfile","AutoProfileStateFile","AutoProfileProfiles","AutoProfileMinHoldSeconds",
    "AutoProfileSwitchMargin","AutoProfileSwitchbackCooldownSeconds","AutoProfileRecheckSeconds",
    "AutoProfileBinaryPath","StartGraceSeconds","UpgradeHealthSeconds","RuntimeTemplateFile",
    "RestartAfterSetRuntime","NodeGroup","UpgradeWindow","RequireNodeGroup","Force"
)

function Resolve-NovovmCtlBinary {
    if (-not [string]::IsNullOrWhiteSpace($CtlBinaryFile)) {
        if (Test-Path -LiteralPath $CtlBinaryFile) { return $CtlBinaryFile }
        throw ("novovmctl explicit path not found: " + $CtlBinaryFile)
    }

    $explicitEnv = [Environment]::GetEnvironmentVariable("NOVOVMCTL_BINARY")
    if (-not [string]::IsNullOrWhiteSpace($explicitEnv)) {
        if (Test-Path -LiteralPath $explicitEnv) { return $explicitEnv }
        throw ("novovmctl NOVOVMCTL_BINARY not found: " + $explicitEnv)
    }

    $candidates = New-Object System.Collections.Generic.List[string]

    $cargoTargetDir = [Environment]::GetEnvironmentVariable("CARGO_TARGET_DIR")
    if (-not [string]::IsNullOrWhiteSpace($cargoTargetDir)) {
        $candidates.Add((Join-Path $cargoTargetDir "release/novovmctl.exe"))
        $candidates.Add((Join-Path $cargoTargetDir "release/novovmctl"))
        $candidates.Add((Join-Path $cargoTargetDir "debug/novovmctl.exe"))
        $candidates.Add((Join-Path $cargoTargetDir "debug/novovmctl"))
    }

    $candidates.Add("D:/cargo-target-supervm/release/novovmctl.exe")
    $candidates.Add("D:/cargo-target-supervm/release/novovmctl")
    $candidates.Add("D:/cargo-target-supervm/debug/novovmctl.exe")
    $candidates.Add("D:/cargo-target-supervm/debug/novovmctl")

    $repoRoot = Resolve-CompatRepoRoot
    $candidates.Add((Join-Path $repoRoot "target/release/novovmctl.exe"))
    $candidates.Add((Join-Path $repoRoot "target/release/novovmctl"))
    $candidates.Add((Join-Path $repoRoot "target/debug/novovmctl.exe"))
    $candidates.Add((Join-Path $repoRoot "target/debug/novovmctl"))
    $candidates.Add((Join-Path $repoRoot "crates/novovmctl/target/release/novovmctl.exe"))
    $candidates.Add((Join-Path $repoRoot "crates/novovmctl/target/release/novovmctl"))
    $candidates.Add((Join-Path $repoRoot "crates/novovmctl/target/debug/novovmctl.exe"))
    $candidates.Add((Join-Path $repoRoot "crates/novovmctl/target/debug/novovmctl"))

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) { return $candidate }
    }
    throw "novovmctl not found in default candidate paths"
}

function Resolve-CompatRepoRoot {
    if (-not [string]::IsNullOrWhiteSpace($RepoRoot)) { return $RepoRoot }
    return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

$unsupported = @($PSBoundParameters.Keys | Where-Object { $script:NovovmCtlLifecycleSupportedParams -notcontains $_ })
if ($unsupported.Count -gt 0) {
    throw ("novovm-node-lifecycle.ps1 compatibility shell only supports novovmctl-backed parameters. Unsupported legacy parameters: " + ($unsupported -join ", "))
}
$script:CompatBoundParameters = @{}
foreach ($entry in $PSBoundParameters.GetEnumerator()) {
    $script:CompatBoundParameters[$entry.Key] = $entry.Value
}

$novovmctl = Resolve-NovovmCtlBinary
$argsList = New-Object System.Collections.Generic.List[string]
$argsList.Add("lifecycle")
$argsList.Add("--action")
$argsList.Add($Action)
$argsList.Add("--repo-root")
$argsList.Add((Resolve-CompatRepoRoot))

function Add-ArgPair([string]$Flag, [string]$Value) {
    if (-not [string]::IsNullOrWhiteSpace($Value)) {
        $argsList.Add($Flag)
        $argsList.Add($Value)
    }
}

function Add-BoundArgPair([string]$Name, [string]$Flag, [string]$Value) {
    if ($script:CompatBoundParameters.ContainsKey($Name)) {
        Add-ArgPair $Flag $Value
    }
}

Add-BoundArgPair "Version" "--version" $Version
Add-BoundArgPair "TargetVersion" "--target-version" $TargetVersion
Add-BoundArgPair "RollbackVersion" "--rollback-version" $RollbackVersion
Add-BoundArgPair "GatewayBinaryFrom" "--gateway-binary-from" $GatewayBinaryFrom
Add-BoundArgPair "NodeBinaryFrom" "--node-binary-from" $NodeBinaryFrom
if ($SetCurrent) { $argsList.Add("--set-current") }
Add-ArgPair "--release-root" $ReleaseRoot
Add-ArgPair "--runtime-state-file" $RuntimeStateFile
Add-BoundArgPair "AuditFile" "--audit-file" $AuditFile
Add-ArgPair "--runtime-pid-file" $RuntimePidFile
Add-ArgPair "--runtime-log-dir" $RuntimeLogDir
Add-ArgPair "--profile" $Profile
Add-ArgPair "--role-profile" $RoleProfile
Add-BoundArgPair "OverlayRouteMode" "--overlay-route-mode" $OverlayRouteMode
Add-BoundArgPair "OverlayRouteRuntimeFile" "--overlay-route-runtime-file" $OverlayRouteRuntimeFile
Add-BoundArgPair "OverlayRouteRuntimeProfile" "--overlay-route-runtime-profile" $OverlayRouteRuntimeProfile
Add-BoundArgPair "OverlayRouteRelayDirectoryFile" "--overlay-route-relay-directory-file" $OverlayRouteRelayDirectoryFile
if ($UseNodeWatchMode) { $argsList.Add("--use-node-watch-mode") }
if ($PSBoundParameters.ContainsKey("PollMs") -and $PollMs -gt 0) { Add-ArgPair "--poll-ms" ([string]$PollMs) }
if ($PSBoundParameters.ContainsKey("NodeWatchBatchMaxFiles") -and $NodeWatchBatchMaxFiles -gt 0) { Add-ArgPair "--node-watch-batch-max-files" ([string]$NodeWatchBatchMaxFiles) }
if ($PSBoundParameters.ContainsKey("IdleExitSeconds") -and $IdleExitSeconds -ge 0) { Add-ArgPair "--idle-exit-seconds" ([string]$IdleExitSeconds) }
if ($EnableAutoProfile) { $argsList.Add("--auto-profile-enabled") }
Add-BoundArgPair "AutoProfileStateFile" "--auto-profile-state-file" $AutoProfileStateFile
Add-BoundArgPair "AutoProfileProfiles" "--auto-profile-profiles" $AutoProfileProfiles
if ($PSBoundParameters.ContainsKey("AutoProfileMinHoldSeconds") -and $AutoProfileMinHoldSeconds -gt 0) { Add-ArgPair "--auto-profile-min-hold-seconds" ([string]$AutoProfileMinHoldSeconds) }
if ($PSBoundParameters.ContainsKey("AutoProfileSwitchMargin")) { Add-ArgPair "--auto-profile-switch-margin" ([string]$AutoProfileSwitchMargin) }
if ($PSBoundParameters.ContainsKey("AutoProfileSwitchbackCooldownSeconds") -and $AutoProfileSwitchbackCooldownSeconds -gt 0) { Add-ArgPair "--auto-profile-switchback-cooldown-seconds" ([string]$AutoProfileSwitchbackCooldownSeconds) }
if ($PSBoundParameters.ContainsKey("AutoProfileRecheckSeconds") -and $AutoProfileRecheckSeconds -gt 0) { Add-ArgPair "--auto-profile-recheck-seconds" ([string]$AutoProfileRecheckSeconds) }
Add-BoundArgPair "AutoProfileBinaryPath" "--policy-cli-binary-file" $AutoProfileBinaryPath
Add-BoundArgPair "RuntimeTemplateFile" "--runtime-template-file" $RuntimeTemplateFile
if ($RestartAfterSetRuntime) { $argsList.Add("--restart-after-set-runtime") }
if ($PSBoundParameters.ContainsKey("StartGraceSeconds") -and $StartGraceSeconds -gt 0) { Add-ArgPair "--start-grace-seconds" ([string]$StartGraceSeconds) }
if ($PSBoundParameters.ContainsKey("UpgradeHealthSeconds") -and $UpgradeHealthSeconds -gt 0) { Add-ArgPair "--upgrade-health-seconds" ([string]$UpgradeHealthSeconds) }
Add-BoundArgPair "NodeGroup" "--node-group" $NodeGroup
Add-BoundArgPair "UpgradeWindow" "--upgrade-window" $UpgradeWindow
Add-BoundArgPair "RequireNodeGroup" "--require-node-group" $RequireNodeGroup
if ($Force) { $argsList.Add("--force") }

Write-Host ("[compat-shell] forwarding to novovmctl: {0} {1}" -f $novovmctl, ($argsList -join " "))
& $novovmctl @argsList
exit $LASTEXITCODE
