[CmdletBinding()]
param(
    [string]$RepoRoot = "",
    [string]$OutputDir = "artifacts/deploy/role-matrix",
    [string]$L1NodeId = "novovm-l1-01",
    [string]$L2NodeId = "novovm-l2-01",
    [string]$L3NodeId = "novovm-l3-01",
    [string]$L3GatewayBind = "0.0.0.0:9899",
    [string]$SpoolDir = "artifacts/ingress/spool",
    [ValidateRange(10, 60000)]
    [int]$PollMs = 100,
    [ValidateRange(50, 60000)]
    [int]$SupervisorPollMs = 1000,
    [ValidateRange(1, 1000000)]
    [int]$NodeWatchBatchMaxFiles = 2048
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Resolve-RootPath {
    param([string]$Root)
    if (-not $Root) {
        return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
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

function Write-TextFile {
    param(
        [string]$PathValue,
        [string]$Content
    )
    [System.IO.File]::WriteAllText($PathValue, $Content, [System.Text.Encoding]::UTF8)
}

function Build-RoleScript {
    param(
        [string]$NodeId,
        [string]$RoleProfile,
        [string]$ExtraArgs
    )
    $nodeIdEscaped = $NodeId.Replace("'", "''")
    @"
`$env:NOVOVM_NODE_ID = '$nodeIdEscaped'
powershell -ExecutionPolicy Bypass -File .\scripts\novovm-up.ps1 -Profile prod -RoleProfile $RoleProfile -Daemon -SpoolDir $SpoolDir -PollMs $PollMs -SupervisorPollMs $SupervisorPollMs -NodeWatchBatchMaxFiles $NodeWatchBatchMaxFiles $ExtraArgs
"@
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$outDirFull = Resolve-FullPath -Root $RepoRoot -Value $OutputDir
New-Item -ItemType Directory -Force -Path $outDirFull | Out-Null

$l1Script = Build-RoleScript -NodeId $L1NodeId -RoleProfile "l1" -ExtraArgs ""
$l2Script = Build-RoleScript -NodeId $L2NodeId -RoleProfile "l2" -ExtraArgs ""
$l3Script = Build-RoleScript -NodeId $L3NodeId -RoleProfile "l3" -ExtraArgs ("-GatewayBind " + $L3GatewayBind)
$fullScript = Build-RoleScript -NodeId "novovm-full-01" -RoleProfile "full" -ExtraArgs ("-GatewayBind " + $L3GatewayBind)

Write-TextFile -PathValue (Join-Path $outDirFull "run-l1.ps1") -Content $l1Script
Write-TextFile -PathValue (Join-Path $outDirFull "run-l2.ps1") -Content $l2Script
Write-TextFile -PathValue (Join-Path $outDirFull "run-l3.ps1") -Content $l3Script
Write-TextFile -PathValue (Join-Path $outDirFull "run-full.ps1") -Content $fullScript

$readme = @"
NOVOVM role matrix launch scripts
generated_at: $(Get-Date -Format "yyyy-MM-dd HH:mm:ss")
repo_root: $RepoRoot
output_dir: $outDirFull

usage:
1) copy run-l1.ps1 to L1 host and execute
2) copy run-l2.ps1 to L2 host and execute
3) copy run-l3.ps1 to L3 host and execute
4) or use run-full.ps1 for single-host full mode

notes:
- all scripts use production profile and daemon mode
- node ids are prefilled and can be edited
- spool, polling and watch batch parameters are fixed in generated scripts
"@
Write-TextFile -PathValue (Join-Path $outDirFull "README.txt") -Content $readme

Write-Host ("role_matrix_out: " + $outDirFull)
