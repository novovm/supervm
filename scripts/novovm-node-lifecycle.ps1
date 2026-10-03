<#
DEPRECATED COMPATIBILITY SHELL

Mainline production entry is `novovmctl`.
This shell only forwards parameters to `novovmctl lifecycle`.
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
if (-not (Test-ArgPresent -Args $ForwardArgs -Names @("-Action", "--action"))) {
    $baseArgs += @("--action", "status")
}
if (-not (Test-ArgPresent -Args $ForwardArgs -Names @("-RepoRoot", "--repo-root"))) {
    $baseArgs += @("--repo-root", $repoRoot)
}

Invoke-NovovmctlForward -RepoRoot $repoRoot -Subcommand "lifecycle" -BaseArgs $baseArgs -IncomingArgs $ForwardArgs
