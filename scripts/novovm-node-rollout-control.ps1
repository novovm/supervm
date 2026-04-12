<#
DEPRECATED COMPATIBILITY SHELL

Mainline production entry is `novovmctl`.
This shell only forwards parameters to `novovmctl rollout-control`.
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ForwardArgs
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
. (Join-Path $PSScriptRoot "_compat\Invoke-NovovmctlForward.ps1")

function Test-ArgPresent([string[]]$Args, [string[]]$Names) {
    foreach ($tok in $Args) {
        foreach ($name in $Names) {
            if ($tok -eq $name) { return $true }
        }
    }
    return $false
}

$baseArgs = @()
if (-not (Test-ArgPresent -Args $ForwardArgs -Names @("-QueueFile", "--queue-file"))) {
    $baseArgs += @("--queue-file", "config/runtime/lifecycle/rollout.queue.json")
}
if (-not (Test-ArgPresent -Args $ForwardArgs -Names @("-PlanAction", "--plan-action"))) {
    $baseArgs += @("--plan-action", "upgrade")
}
if (-not (Test-ArgPresent -Args $ForwardArgs -Names @("-ControllerId", "--controller-id"))) {
    $baseArgs += @("--controller-id", "ops-main")
}
if (-not (Test-ArgPresent -Args $ForwardArgs -Names @("-AuditFile", "--audit-file"))) {
    $baseArgs += @("--audit-file", "artifacts/runtime/rollout/control-plane-audit.jsonl")
}

Invoke-NovovmctlForward -RepoRoot $repoRoot -Subcommand "rollout-control" -BaseArgs $baseArgs -IncomingArgs $ForwardArgs
