[CmdletBinding()]
param(
    [string]$RepoRoot = "",
    [string]$AnchorFile = "artifacts/l1/l1l4-anchor.jsonl",
    [string]$OutputDir = "artifacts/l1/settlement-cycles",
    [string]$VoucherIndexFile = "artifacts/l1/l1l4-settlement-vouchers.jsonl",
    [string]$CursorFile = "artifacts/l1/l1l4-settlement.cursor",
    [ValidateRange(0, 4294967295)]
    [uint32]$PenaltyFailedFile = 1,
    [ValidateRange(1, 4294967295)]
    [uint32]$RewardPerScoreUnit = 1,
    [switch]$FullReplay,
    [switch]$NoCursorUpdate
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

function To-U64 {
    param($Value)
    if ($null -eq $Value) {
        return [uint64]0
    }
    try {
        return [uint64]$Value
    } catch {
        return [uint64]0
    }
}

function Ensure-ParentDir {
    param([string]$PathValue)
    $parent = Split-Path -Parent $PathValue
    if ($parent) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$anchorPath = Resolve-FullPath -Root $RepoRoot -Value $AnchorFile
$outDir = Resolve-FullPath -Root $RepoRoot -Value $OutputDir
$voucherIndexPath = Resolve-FullPath -Root $RepoRoot -Value $VoucherIndexFile
$cursorPath = Resolve-FullPath -Root $RepoRoot -Value $CursorFile

if (-not (Test-Path -LiteralPath $anchorPath)) {
    throw ("anchor file not found: " + $anchorPath)
}

$cursorTs = [uint64]0
if (-not $FullReplay -and (Test-Path -LiteralPath $cursorPath)) {
    $rawCursor = (Get-Content -LiteralPath $cursorPath -Raw).Trim()
    if ($rawCursor) {
        try {
            $cursorTs = [uint64]$rawCursor
        } catch {
            throw ("invalid cursor file content at " + $cursorPath + ": " + $rawCursor)
        }
    }
}

$nodes = @{}
$anchorCount = [uint64]0
$windowFrom = [uint64]0
$windowTo = [uint64]0

$lineNo = 0
foreach ($line in Get-Content -LiteralPath $anchorPath) {
    $lineNo = $lineNo + 1
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $item = $null
    try {
        $item = $line | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw ("invalid json at line " + $lineNo + " in " + $anchorPath + ": " + $_.Exception.Message)
    }

    $ts = To-U64 $item.ts_unix_ms
    if (-not $FullReplay -and $ts -le $cursorTs) {
        continue
    }

    $nodeId = [string]$item.node_id
    if ([string]::IsNullOrWhiteSpace($nodeId)) {
        $nodeId = "unknown"
    }
    if (-not $nodes.ContainsKey($nodeId)) {
        $nodes[$nodeId] = [ordered]@{
            node_id = $nodeId
            l4_ingress_ops = [uint64]0
            l3_routed_batches = [uint64]0
            l2_exec_ok_ops = [uint64]0
            l2_exec_failed_files = [uint64]0
        }
    }
    $acc = $nodes[$nodeId]
    $acc.l4_ingress_ops = [uint64]($acc.l4_ingress_ops + (To-U64 $item.l4_ingress_ops))
    $acc.l3_routed_batches = [uint64]($acc.l3_routed_batches + (To-U64 $item.l3_routed_batches))
    $acc.l2_exec_ok_ops = [uint64]($acc.l2_exec_ok_ops + (To-U64 $item.l2_exec_ok_ops))
    $acc.l2_exec_failed_files = [uint64]($acc.l2_exec_failed_files + (To-U64 $item.l2_exec_failed_files))

    $anchorCount = [uint64]($anchorCount + 1)
    if ($windowFrom -eq 0 -or $ts -lt $windowFrom) {
        $windowFrom = $ts
    }
    if ($ts -gt $windowTo) {
        $windowTo = $ts
    }
}

if ($anchorCount -eq 0) {
    Write-Host ("l1l4_settlement_cycle_out: no_new_anchor_records cursor_ts=" + $cursorTs)
    exit 0
}

$totalScore = [uint64]0
$totalReward = [uint64]0
$nodeRows = @()
$sortedKeys = @($nodes.Keys | Sort-Object)

foreach ($key in $sortedKeys) {
    $row = $nodes[$key]
    $rawScore = [int64]$row.l4_ingress_ops + [int64]$row.l3_routed_batches + [int64]$row.l2_exec_ok_ops - ([int64]$PenaltyFailedFile * [int64]$row.l2_exec_failed_files)
    if ($rawScore -lt 0) {
        $rawScore = 0
    }
    $score = [uint64]$rawScore
    $reward = [uint64]($score * [uint64]$RewardPerScoreUnit)
    $row["score"] = $score
    $row["reward_units"] = $reward
    $nodeRows += [pscustomobject]$row
    $totalScore = [uint64]($totalScore + $score)
    $totalReward = [uint64]($totalReward + $reward)
}

if ($totalScore -gt 0) {
    for ($i = 0; $i -lt $nodeRows.Count; $i = $i + 1) {
        $score = [uint64]$nodeRows[$i].score
        $share = [uint64][Math]::Floor(([double]$score * 1000000.0) / [double]$totalScore)
        $nodeRows[$i] | Add-Member -NotePropertyName share_ppm -NotePropertyValue $share
    }
} else {
    for ($i = 0; $i -lt $nodeRows.Count; $i = $i + 1) {
        $nodeRows[$i] | Add-Member -NotePropertyName share_ppm -NotePropertyValue ([uint64]0)
    }
}

$nowMs = [uint64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$voucherId = ("l1s{0:x16}{1:x16}{2:x8}" -f $windowFrom, $windowTo, [uint64]$anchorCount)

$voucher = [ordered]@{
    version = 1
    voucher_id = $voucherId
    created_at_unix_ms = $nowMs
    window_from_unix_ms = $windowFrom
    window_to_unix_ms = $windowTo
    anchor_count = $anchorCount
    node_count = [uint64]$nodeRows.Count
    reward_per_score_unit = [uint64]$RewardPerScoreUnit
    penalty_failed_file = [uint64]$PenaltyFailedFile
    total_score = $totalScore
    total_reward_units = $totalReward
    source_anchor_file = $anchorPath
    nodes = $nodeRows
}

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$voucherPath = Join-Path $outDir ($voucherId + ".json")
$voucherJson = $voucher | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($voucherPath, $voucherJson, [System.Text.Encoding]::UTF8)

Ensure-ParentDir -PathValue $voucherIndexPath
$voucherIndexLine = ($voucher | ConvertTo-Json -Depth 8 -Compress) + "`n"
[System.IO.File]::AppendAllText($voucherIndexPath, $voucherIndexLine, [System.Text.Encoding]::UTF8)

if (-not $NoCursorUpdate) {
    Ensure-ParentDir -PathValue $cursorPath
    [System.IO.File]::WriteAllText($cursorPath, $windowTo.ToString(), [System.Text.Encoding]::UTF8)
}

Write-Host ("l1l4_settlement_cycle_out: voucher_id=" + $voucherId + " anchor_count=" + $anchorCount + " node_count=" + $nodeRows.Count + " total_score=" + $totalScore + " total_reward_units=" + $totalReward + " voucher_path=" + $voucherPath)
