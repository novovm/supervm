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
    [string]$CtlBinaryFile,
    [ValidateSet("prod", "dev")]
    [string]$Profile = "prod",
    [ValidateSet("full", "l1", "l2", "l3")]
    [string]$RoleProfile = "full",
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
    [int]$MaxRestarts = 0,
    [switch]$DryRun,
    [string]$AuditFile,
    [string]$LogFile,
    [string]$PolicyCliBinaryFile,
    [string]$NodeBinaryFile
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# COMPATIBILITY SHELL ONLY
# This script must not contain policy logic, rollout logic, risk evaluation,
# failover logic, or runtime decision logic on the mainline path.
# Mainline execution must go through `novovmctl daemon`.
# PowerShell here is limited to parameter forwarding and exit-code passthrough.

function Resolve-NovovmCtlBinary {
    if ($CtlBinaryFile) {
        if (Test-Path -LiteralPath $CtlBinaryFile) {
            return $CtlBinaryFile
        }
        throw "explicit novovmctl path not found: $CtlBinaryFile"
    }

    $candidates = New-Object System.Collections.Generic.List[string]
    if ($env:NOVOVMCTL_BINARY) {
        $candidates.Add($env:NOVOVMCTL_BINARY)
    }
    if ($env:CARGO_TARGET_DIR) {
        $candidates.Add((Join-Path $env:CARGO_TARGET_DIR "release/novovmctl.exe"))
        $candidates.Add((Join-Path $env:CARGO_TARGET_DIR "debug/novovmctl.exe"))
        $candidates.Add((Join-Path $env:CARGO_TARGET_DIR "release/novovmctl"))
        $candidates.Add((Join-Path $env:CARGO_TARGET_DIR "debug/novovmctl"))
    }
    $candidates.Add("D:/cargo-target-supervm/release/novovmctl.exe")
    $candidates.Add("D:/cargo-target-supervm/debug/novovmctl.exe")
    $candidates.Add("D:/cargo-target-supervm/release/novovmctl")
    $candidates.Add("D:/cargo-target-supervm/debug/novovmctl")
    foreach ($candidate in @(
        "target/release/novovmctl.exe",
        "target/release/novovmctl",
        "target/debug/novovmctl.exe",
        "target/debug/novovmctl",
        "crates/novovmctl/target/release/novovmctl.exe",
        "crates/novovmctl/target/release/novovmctl",
        "crates/novovmctl/target/debug/novovmctl.exe",
        "crates/novovmctl/target/debug/novovmctl"
    )) {
        $candidates.Add($candidate)
    }
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    throw "novovmctl not found in default candidate paths"
}

function Invoke-NovovmCtlDaemonBridge {
    $novovmctl = Resolve-NovovmCtlBinary
    $argsList = New-Object System.Collections.Generic.List[string]
    $argsList.Add("daemon")
    $argsList.Add("--profile")
    $argsList.Add($Profile)
    $argsList.Add("--role-profile")
    $argsList.Add($RoleProfile)
    $argsList.Add("--supervisor-poll-ms")
    $argsList.Add([string]$SupervisorPollMs)
    $argsList.Add("--restart-delay-seconds")
    $argsList.Add([string]$RestartDelaySeconds)
    $argsList.Add("--max-restarts")
    $argsList.Add([string]$MaxRestarts)
    $argsList.Add("--poll-ms")
    $argsList.Add([string]$PollMs)
    $argsList.Add("--node-watch-batch-max-files")
    $argsList.Add([string]$NodeWatchBatchMaxFiles)
    $argsList.Add("--idle-exit-seconds")
    $argsList.Add([string]$IdleExitSeconds)
    $argsList.Add("--gateway-bind")
    $argsList.Add($GatewayBind)
    $argsList.Add("--spool-dir")
    $argsList.Add($SpoolDir)
    $argsList.Add("--gateway-max-requests")
    $argsList.Add([string]$GatewayMaxRequests)

    if ($UseNodeWatchMode) {
        $argsList.Add("--use-node-watch-mode")
    }
    if ($NoGateway) {
        $argsList.Add("--no-gateway")
    }
    if ($BuildBeforeRun) {
        $argsList.Add("--build-before-run")
    }
    if ($LeanIo) {
        $argsList.Add("--lean-io")
    }
    if ($DryRun) {
        $argsList.Add("--dry-run")
    }
    if ($AuditFile) {
        $argsList.Add("--audit-file")
        $argsList.Add($AuditFile)
    }
    if ($LogFile) {
        $argsList.Add("--log-file")
        $argsList.Add($LogFile)
    }
    if ($PolicyCliBinaryFile) {
        $argsList.Add("--policy-cli-binary-file")
        $argsList.Add($PolicyCliBinaryFile)
    }
    if ($NodeBinaryFile) {
        $argsList.Add("--node-binary-file")
        $argsList.Add($NodeBinaryFile)
    }

    Write-Host ("[compat-shell] forwarding to novovmctl: {0} {1}" -f $novovmctl, ($argsList -join " "))
    & $novovmctl @argsList
    exit $LASTEXITCODE
}

Invoke-NovovmCtlDaemonBridge
