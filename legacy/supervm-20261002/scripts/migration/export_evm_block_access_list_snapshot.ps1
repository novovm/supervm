<#
Thin operational snapshot shell.

Purpose:
- export one EVM block access list snapshot to a JSON artifact
- keep this as a direct production-use tool, not a broader engineering surface

Examples:
- powershell -File scripts/migration/export_evm_block_access_list_snapshot.ps1
- powershell -File scripts/migration/export_evm_block_access_list_snapshot.ps1 -BlockHash 0x...
- powershell -File scripts/migration/export_evm_block_access_list_snapshot.ps1 -BlockNumber 0x123 -RequirePayload
#>
[CmdletBinding(PositionalBinding = $false)]
param(
    [string]$RepoRoot = "",
    [string]$BlockHash = "",
    [string]$BlockNumber = "latest",
    [string]$StorePath = "",
    [string]$OutPath = "",
    [switch]$RequirePayload,
    [switch]$RequireComplete
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
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

function Normalize-SelectorForFileName {
    param(
        [string]$Hash,
        [string]$Number
    )
    if (-not [string]::IsNullOrWhiteSpace($Hash)) {
        $trimmed = $Hash.Trim()
        $trimmed = $trimmed -replace '^0x', ''
        $trimmed = $trimmed -replace '^0X', ''
        if ($trimmed.Length -gt 16) {
            $trimmed = $trimmed.Substring(0, 16)
        }
        return ("hash-{0}" -f $trimmed.ToLowerInvariant())
    }
    $normalized = $Number.Trim() -replace '[^A-Za-z0-9_-]', '_'
    if ([string]::IsNullOrWhiteSpace($normalized)) {
        $normalized = "latest"
    }
    return ("number-{0}" -f $normalized)
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$selectorLabel = Normalize-SelectorForFileName -Hash $BlockHash -Number $BlockNumber
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
if ([string]::IsNullOrWhiteSpace($OutPath)) {
    $OutPath = "artifacts/migration/evm-bal-snapshots/{0}-{1}.json" -f $selectorLabel, $stamp
}
$outAbs = Resolve-FullPath -Root $RepoRoot -Value $OutPath
$outDir = Split-Path -Parent $outAbs
if (-not [string]::IsNullOrWhiteSpace($outDir)) {
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
}

$baseArgs = @("--json-out", $outAbs)
if (-not [string]::IsNullOrWhiteSpace($StorePath)) {
    $storeAbs = Resolve-FullPath -Root $RepoRoot -Value $StorePath
    $baseArgs += @("--store-path", $storeAbs)
}
if (-not [string]::IsNullOrWhiteSpace($BlockHash)) {
    $baseArgs += @("--block-hash", $BlockHash.Trim())
} else {
    $baseArgs += @("--block-number", $BlockNumber.Trim())
}
if ($RequirePayload) {
    $baseArgs += "--require-payload"
}
if ($RequireComplete) {
    $baseArgs += "--require-complete"
}

Write-Host ("evm_bal_snapshot_out={0}" -f $outAbs)

. (Join-Path $PSScriptRoot "..\_compat\Invoke-NovovmctlForward.ps1")
Invoke-NovovmctlForward -RepoRoot $RepoRoot -Subcommand "evm-block-access-list" -BaseArgs $baseArgs -IncomingArgs @()
