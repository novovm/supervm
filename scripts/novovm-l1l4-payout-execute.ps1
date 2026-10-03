[CmdletBinding()]
param(
    [string]$RepoRoot = "",
    [string]$DispatchIndexFile = "artifacts/l1/l1l4-payout-dispatch.jsonl",
    [string]$OutputDir = "artifacts/l1/payout-executed",
    [string]$ExecutedIndexFile = "artifacts/l1/l1l4-payout-executed.jsonl",
    [string]$CursorFile = "artifacts/l1/l1l4-payout-execute.cursor",
    [UInt64]$ChainId = 1,
    [string]$ExecutionMode = "ledger_status_only_v1",
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

function Ensure-ParentDir {
    param([string]$PathValue)
    $parent = Split-Path -Parent $PathValue
    if ($parent) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
}

function To-U64 {
    param($Value)
    if ($null -eq $Value) {
        return [UInt64]0
    }
    try {
        return [UInt64]$Value
    } catch {
        return [UInt64]0
    }
}

function New-PseudoTxHashHex {
    param(
        [string]$VoucherId,
        [string]$PayoutId,
        [UInt64]$NowMs
    )
    $seed = "$VoucherId|$PayoutId|$NowMs"
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($seed)
        $hash = $sha.ComputeHash($bytes)
        return -join ($hash | ForEach-Object { $_.ToString("x2") })
    } finally {
        $sha.Dispose()
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$dispatchIndexPath = Resolve-FullPath -Root $RepoRoot -Value $DispatchIndexFile
$outDir = Resolve-FullPath -Root $RepoRoot -Value $OutputDir
$executedIndexPath = Resolve-FullPath -Root $RepoRoot -Value $ExecutedIndexFile
$cursorPath = Resolve-FullPath -Root $RepoRoot -Value $CursorFile

if (-not (Test-Path -LiteralPath $dispatchIndexPath)) {
    throw ("dispatch index file not found: " + $dispatchIndexPath)
}

$cursorDispatchAt = [UInt64]0
if (-not $FullReplay -and (Test-Path -LiteralPath $cursorPath)) {
    $rawCursor = (Get-Content -LiteralPath $cursorPath -Raw).Trim()
    if ($rawCursor) {
        try {
            $cursorDispatchAt = [UInt64]$rawCursor
        } catch {
            throw ("invalid execute cursor at " + $cursorPath + ": " + $rawCursor)
        }
    }
}

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Ensure-ParentDir -PathValue $executedIndexPath

$processedDispatches = 0
$executedCount = 0
$lastDispatchAt = $cursorDispatchAt

$lineNo = 0
foreach ($line in Get-Content -LiteralPath $dispatchIndexPath) {
    $lineNo = $lineNo + 1
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $dispatch = $null
    try {
        $dispatch = $line | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw ("invalid dispatch json at line " + $lineNo + " in " + $dispatchIndexPath + ": " + $_.Exception.Message)
    }

    $dispatchAt = To-U64 $dispatch.dispatch_created_at_unix_ms
    if (-not $FullReplay -and $dispatchAt -le $cursorDispatchAt) {
        continue
    }

    $voucherId = [string]$dispatch.voucher_id
    if ([string]::IsNullOrWhiteSpace($voucherId)) {
        throw ("voucher_id missing at line " + $lineNo + " in " + $dispatchIndexPath)
    }

    $rows = @()
    $instructions = @($dispatch.payout_instructions)
    foreach ($inst in $instructions) {
        $payoutId = [string]$inst.payout_id
        $nodeId = [string]$inst.node_id
        $payoutAccount = [string]$inst.payout_account
        $rewardUnits = To-U64 $inst.reward_units
        $nowMs = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $txHashHex = New-PseudoTxHashHex -VoucherId $voucherId -PayoutId $payoutId -NowMs $nowMs

        $rows += [pscustomobject][ordered]@{
            version = 1
            voucher_id = $voucherId
            payout_id = $payoutId
            node_id = $nodeId
            payout_account = $payoutAccount
            reward_units = $rewardUnits
            chain_id = $ChainId
            tx_hash_hex = $txHashHex
            execution_mode = $ExecutionMode
            status = "credited_v1"
            executed_at_unix_ms = $nowMs
        }
    }

    $executedVoucher = [ordered]@{
        version = 1
        voucher_id = $voucherId
        dispatch_created_at_unix_ms = $dispatchAt
        executed_created_at_unix_ms = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        chain_id = $ChainId
        execution_mode = $ExecutionMode
        execute_count = [UInt64]$rows.Count
        payout_executions = $rows
    }

    $voucherOutPath = Join-Path $outDir ($voucherId + ".executed.json")
    [System.IO.File]::WriteAllText(
        $voucherOutPath,
        ($executedVoucher | ConvertTo-Json -Depth 8),
        [System.Text.Encoding]::UTF8
    )

    $execLine = ($executedVoucher | ConvertTo-Json -Depth 8 -Compress) + "`n"
    [System.IO.File]::AppendAllText($executedIndexPath, $execLine, [System.Text.Encoding]::UTF8)

    $processedDispatches = $processedDispatches + 1
    $executedCount = $executedCount + $rows.Count
    if ($dispatchAt -gt $lastDispatchAt) {
        $lastDispatchAt = $dispatchAt
    }
}

if ($processedDispatches -eq 0) {
    Write-Host ("l1l4_payout_execute_out: no_new_dispatch cursor_dispatch_at=" + $cursorDispatchAt)
    exit 0
}

if (-not $NoCursorUpdate) {
    Ensure-ParentDir -PathValue $cursorPath
    [System.IO.File]::WriteAllText($cursorPath, $lastDispatchAt.ToString(), [System.Text.Encoding]::UTF8)
}

Write-Host ("l1l4_payout_execute_out: processed_dispatches=" + $processedDispatches + " executed=" + $executedCount + " executed_index=" + $executedIndexPath)
