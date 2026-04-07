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
    [ValidateSet("upgrade", "rollback", "status", "set-policy")]
    [string]$Action = "upgrade",
    [string]$CtlBinaryFile = "",
    [string]$PlanFile = "config/runtime/lifecycle/rollout.plan.json",
    [string]$TargetVersion = "",
    [string]$RollbackVersion = "",
    [string[]]$GroupOrder = @("canary", "stable"),
    [ValidateRange(1, 600)]
    [int]$UpgradeHealthSeconds = 12,
    [ValidateRange(0, 100)]
    [int]$DefaultMaxFailures = 0,
    [ValidateRange(0, 600)]
    [int]$PauseSecondsBetweenNodes = 3,
    [ValidateSet("local", "ssh", "winrm")]
    [string]$DefaultTransport = "local",
    [string]$SshBinary = "ssh",
    [string]$SshIdentityFile = "",
    [string]$SshKnownHostsFile = "",
    [ValidateSet("accept-new", "yes", "no")]
    [string]$SshStrictHostKeyChecking = "accept-new",
    [ValidateRange(1, 3600)]
    [int]$RemoteTimeoutSeconds = 30,
    [string]$RemoteShell = "powershell",
    [string]$WinRmCredentialUserEnv = "",
    [string]$WinRmCredentialPasswordEnv = "",
    [string]$ControllerId = "local-controller",
    [string]$OperationId = "",
    [string]$AuditFile = "artifacts/runtime/rollout/audit.jsonl",
    [ValidateSet("", "secure", "fast")]
    [string]$OverlayRouteMode = "",
    [string]$OverlayRouteRuntimeFile = "",
    [string]$OverlayRouteRuntimeProfile = "",
    [string]$OverlayRouteRelayDirectoryFile = "",
    [ValidateRange(0, 1)]
    [double]$OverlayRouteRelayHealthMin = 0,
    [string]$OverlayRouteRelayPenaltyStateFile = "",
    [string]$OverlayRouteRelayPenaltyDelta = "",
    [ValidateRange(0, 1)]
    [double]$OverlayRouteRelayPenaltyRecoverPerRun = 0.05,
    [string]$OverlayRouteRelayCandidates = "",
    [string]$OverlayRouteRelayCandidatesByRegion = "",
    [string]$OverlayRouteRelayCandidatesByRole = "",
    [switch]$EnableAutoProfile,
    [string]$AutoProfileStateFile = "artifacts/runtime/lifecycle/overlay.auto-profile.state.json",
    [string]$AutoProfileProfiles = "prod-cn,prod-eu,prod-us",
    [ValidateRange(1, 86400)]
    [int]$AutoProfileMinHoldSeconds = 180,
    [ValidateRange(0, 1)]
    [double]$AutoProfileSwitchMargin = 0.08,
    [ValidateRange(1, 86400)]
    [int]$AutoProfileSwitchbackCooldownSeconds = 300,
    [ValidateRange(1, 3600)]
    [int]$AutoProfileRecheckSeconds = 30,
    [string]$AutoProfileBinaryPath = "",
    [switch]$IgnoreUpgradeWindow,
    [switch]$AutoRollbackOnFailure,
    [switch]$ContinueOnFailure,
    [switch]$PrintEffectivePlan,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# COMPATIBILITY SHELL ONLY
# Mainline execution must go through `novovmctl rollout`.
# PowerShell here is limited to parameter forwarding and exit-code passthrough.

$script:NovovmCtlRolloutSupportedParams = @(
    "Action","CtlBinaryFile","PlanFile","TargetVersion","RollbackVersion","GroupOrder",
    "UpgradeHealthSeconds","DefaultMaxFailures","PauseSecondsBetweenNodes","DefaultTransport",
    "SshBinary","SshIdentityFile","SshKnownHostsFile","SshStrictHostKeyChecking",
    "RemoteTimeoutSeconds","RemoteShell","WinRmCredentialUserEnv","WinRmCredentialPasswordEnv",
    "ControllerId","OperationId","AuditFile","OverlayRouteMode","OverlayRouteRuntimeFile",
    "OverlayRouteRuntimeProfile","OverlayRouteRelayDirectoryFile","EnableAutoProfile",
    "AutoProfileStateFile","AutoProfileProfiles","AutoProfileMinHoldSeconds",
    "AutoProfileSwitchMargin","AutoProfileSwitchbackCooldownSeconds","AutoProfileRecheckSeconds",
    "AutoProfileBinaryPath","IgnoreUpgradeWindow","AutoRollbackOnFailure","ContinueOnFailure",
    "PrintEffectivePlan","DryRun"
)

function Resolve-CompatRepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

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

$unsupported = @($PSBoundParameters.Keys | Where-Object { $script:NovovmCtlRolloutSupportedParams -notcontains $_ })
if ($unsupported.Count -gt 0) {
    throw ("novovm-node-rollout.ps1 compatibility shell only supports novovmctl-backed parameters. Unsupported legacy parameters: " + ($unsupported -join ", "))
}
$script:CompatBoundParameters = @{}
foreach ($entry in $PSBoundParameters.GetEnumerator()) {
    $script:CompatBoundParameters[$entry.Key] = $entry.Value
}

$novovmctl = Resolve-NovovmCtlBinary
$argsList = New-Object System.Collections.Generic.List[string]
$argsList.Add("rollout")
$argsList.Add("--action")
$argsList.Add($Action)
$argsList.Add("--plan-file")
$argsList.Add($PlanFile)

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

Add-BoundArgPair "TargetVersion" "--target-version" $TargetVersion
Add-BoundArgPair "RollbackVersion" "--rollback-version" $RollbackVersion
if ($script:CompatBoundParameters.ContainsKey("GroupOrder") -and $null -ne $GroupOrder -and $GroupOrder.Count -gt 0) {
    Add-ArgPair "--group-order" (($GroupOrder | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) -join ",")
}
if ($script:CompatBoundParameters.ContainsKey("UpgradeHealthSeconds")) { Add-ArgPair "--upgrade-health-seconds" ([string]$UpgradeHealthSeconds) }
if ($script:CompatBoundParameters.ContainsKey("DefaultMaxFailures")) { Add-ArgPair "--default-max-failures" ([string]$DefaultMaxFailures) }
if ($script:CompatBoundParameters.ContainsKey("PauseSecondsBetweenNodes")) { Add-ArgPair "--pause-seconds-between-nodes" ([string]$PauseSecondsBetweenNodes) }
Add-BoundArgPair "DefaultTransport" "--default-transport" $DefaultTransport
Add-BoundArgPair "SshBinary" "--ssh-binary" $SshBinary
Add-BoundArgPair "SshIdentityFile" "--ssh-identity-file" $SshIdentityFile
Add-BoundArgPair "SshKnownHostsFile" "--ssh-known-hosts-file" $SshKnownHostsFile
Add-BoundArgPair "SshStrictHostKeyChecking" "--ssh-strict-host-key-checking" $SshStrictHostKeyChecking
if ($script:CompatBoundParameters.ContainsKey("RemoteTimeoutSeconds")) { Add-ArgPair "--remote-timeout-seconds" ([string]$RemoteTimeoutSeconds) }
Add-BoundArgPair "RemoteShell" "--remote-shell" $RemoteShell
Add-BoundArgPair "WinRmCredentialUserEnv" "--winrm-credential-user-env" $WinRmCredentialUserEnv
Add-BoundArgPair "WinRmCredentialPasswordEnv" "--winrm-credential-password-env" $WinRmCredentialPasswordEnv
Add-BoundArgPair "ControllerId" "--controller-id" $ControllerId
Add-BoundArgPair "OperationId" "--operation-id" $OperationId
Add-BoundArgPair "AuditFile" "--audit-file" $AuditFile
Add-BoundArgPair "OverlayRouteMode" "--overlay-route-mode" $OverlayRouteMode
Add-BoundArgPair "OverlayRouteRuntimeFile" "--overlay-route-runtime-file" $OverlayRouteRuntimeFile
Add-BoundArgPair "OverlayRouteRuntimeProfile" "--overlay-route-runtime-profile" $OverlayRouteRuntimeProfile
Add-BoundArgPair "OverlayRouteRelayDirectoryFile" "--overlay-route-relay-directory-file" $OverlayRouteRelayDirectoryFile
if ($EnableAutoProfile) { $argsList.Add("--enable-auto-profile") }
Add-BoundArgPair "AutoProfileStateFile" "--auto-profile-state-file" $AutoProfileStateFile
Add-BoundArgPair "AutoProfileProfiles" "--auto-profile-profiles" $AutoProfileProfiles
if ($script:CompatBoundParameters.ContainsKey("AutoProfileMinHoldSeconds")) { Add-ArgPair "--auto-profile-min-hold-seconds" ([string]$AutoProfileMinHoldSeconds) }
if ($script:CompatBoundParameters.ContainsKey("AutoProfileSwitchMargin")) { Add-ArgPair "--auto-profile-switch-margin" ([string]$AutoProfileSwitchMargin) }
if ($script:CompatBoundParameters.ContainsKey("AutoProfileSwitchbackCooldownSeconds")) { Add-ArgPair "--auto-profile-switchback-cooldown-seconds" ([string]$AutoProfileSwitchbackCooldownSeconds) }
if ($script:CompatBoundParameters.ContainsKey("AutoProfileRecheckSeconds")) { Add-ArgPair "--auto-profile-recheck-seconds" ([string]$AutoProfileRecheckSeconds) }
Add-BoundArgPair "AutoProfileBinaryPath" "--auto-profile-binary-path" $AutoProfileBinaryPath
if ($IgnoreUpgradeWindow) { $argsList.Add("--ignore-upgrade-window") }
if ($AutoRollbackOnFailure) { $argsList.Add("--auto-rollback-on-failure") }
if ($ContinueOnFailure) { $argsList.Add("--continue-on-failure") }
if ($PrintEffectivePlan) { $argsList.Add("--print-effective-plan") }
if ($DryRun) { $argsList.Add("--dry-run") }

Write-Host ("[compat-shell] forwarding to novovmctl: {0} {1}" -f $novovmctl, ($argsList -join " "))
& $novovmctl @argsList
exit $LASTEXITCODE
