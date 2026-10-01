<#
DEPRECATED COMPATIBILITY SHELL

Mainline production entry is `novovmctl`.
This shell only forwards parameters to `novovmctl evm-block-access-list`.
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

Invoke-NovovmctlForward -RepoRoot $repoRoot -Subcommand "evm-block-access-list" -IncomingArgs $ForwardArgs
