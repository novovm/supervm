[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ForwardArgs
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
. (Join-Path $PSScriptRoot "..\_compat\Invoke-NovovmctlForward.ps1")

$baseArgs = @("--use-node-watch-mode")
Invoke-NovovmctlForward -RepoRoot $repoRoot -Subcommand "up" -BaseArgs $baseArgs -IncomingArgs $ForwardArgs
