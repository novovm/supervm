[CmdletBinding()]
param(
    [string]$RepoRoot = "",
    [string]$VoucherIndexFile = "artifacts/l1/l1l4-settlement-vouchers.jsonl",
    [string]$OutputDir = "artifacts/l1/payout-instructions",
    [string]$DispatchIndexFile = "artifacts/l1/l1l4-payout-dispatch.jsonl",
    [string]$CursorFile = "artifacts/l1/l1l4-payout.cursor",
    [string]$PayoutAccountPrefix = "uca:",
    [ValidateRange(1, 18446744073709551615)]
    [UInt64]$MinRewardUnits = 1,
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

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$voucherIndexPath = Resolve-FullPath -Root $RepoRoot -Value $VoucherIndexFile
$outDir = Resolve-FullPath -Root $RepoRoot -Value $OutputDir
$dispatchIndexPath = Resolve-FullPath -Root $RepoRoot -Value $DispatchIndexFile
$cursorPath = Resolve-FullPath -Root $RepoRoot -Value $CursorFile

if (-not (Test-Path -LiteralPath $voucherIndexPath)) {
    throw ("voucher index file not found: " + $voucherIndexPath)
}

$cursorCreatedAt = [UInt64]0
if (-not $FullReplay -and (Test-Path -LiteralPath $cursorPath)) {
    $rawCursor = (Get-Content -LiteralPath $cursorPath -Raw).Trim()
    if ($rawCursor) {
        try {
            $cursorCreatedAt = [UInt64]$rawCursor
        } catch {
            throw ("invalid payout cursor at " + $cursorPath + ": " + $rawCursor)
        }
    }
}

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Ensure-ParentDir -PathValue $dispatchIndexPath

$processedVoucherCount = 0
$dispatchedCount = 0
$skippedCount = 0
$lastCreatedAt = $cursorCreatedAt

$lineNo = 0
foreach ($line in Get-Content -LiteralPath $voucherIndexPath) {
    $lineNo = $lineNo + 1
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $voucher = $null
    try {
        $voucher = $line | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw ("invalid voucher json at line " + $lineNo + " in " + $voucherIndexPath + ": " + $_.Exception.Message)
    }

    $createdAt = To-U64 $voucher.created_at_unix_ms
    if (-not $FullReplay -and $createdAt -le $cursorCreatedAt) {
        continue
    }

    $voucherId = [string]$voucher.voucher_id
    if ([string]::IsNullOrWhiteSpace($voucherId)) {
        throw ("voucher_id missing at line " + $lineNo + " in " + $voucherIndexPath)
    }

    $nodeRows = @($voucher.nodes)
    $rowsOut = @()

    foreach ($row in $nodeRows) {
        $nodeId = [string]$row.node_id
        if ([string]::IsNullOrWhiteSpace($nodeId)) {
            $nodeId = "unknown"
        }
        $rewardUnits = To-U64 $row.reward_units
        if ($rewardUnits -lt $MinRewardUnits) {
            $skippedCount = $skippedCount + 1
            continue
        }
        $sharePpm = To-U64 $row.share_ppm
        $nowMs = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $payoutId = ("l1p{0}{1}" -f $voucherId, ($nodeId -replace "[^a-zA-Z0-9_\-]", "_"))
        $instruction = [ordered]@{
            version = 1
            payout_id = $payoutId
            voucher_id = $voucherId
            node_id = $nodeId
            payout_account = ($PayoutAccountPrefix + $nodeId)
            reward_units = $rewardUnits
            share_ppm = $sharePpm
            window_from_unix_ms = To-U64 $voucher.window_from_unix_ms
            window_to_unix_ms = To-U64 $voucher.window_to_unix_ms
            created_at_unix_ms = $nowMs
            status = "pending_dispatch"
        }
        $rowsOut += [pscustomobject]$instruction
    }

    $voucherDispatch = [ordered]@{
        version = 1
        voucher_id = $voucherId
        voucher_created_at_unix_ms = $createdAt
        dispatch_created_at_unix_ms = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        min_reward_units = [UInt64]$MinRewardUnits
        dispatch_count = [UInt64]$rowsOut.Count
        payout_instructions = $rowsOut
    }

    $voucherOutPath = Join-Path $outDir ($voucherId + ".payout.json")
    [System.IO.File]::WriteAllText(
        $voucherOutPath,
        ($voucherDispatch | ConvertTo-Json -Depth 8),
        [System.Text.Encoding]::UTF8
    )

    $dispatchLine = ($voucherDispatch | ConvertTo-Json -Depth 8 -Compress) + "`n"
    [System.IO.File]::AppendAllText($dispatchIndexPath, $dispatchLine, [System.Text.Encoding]::UTF8)

    $processedVoucherCount = $processedVoucherCount + 1
    $dispatchedCount = $dispatchedCount + $rowsOut.Count
    if ($createdAt -gt $lastCreatedAt) {
        $lastCreatedAt = $createdAt
    }
}

if ($processedVoucherCount -eq 0) {
    Write-Host ("l1l4_auto_payout_out: no_new_voucher cursor_created_at=" + $cursorCreatedAt)
    exit 0
}

if (-not $NoCursorUpdate) {
    Ensure-ParentDir -PathValue $cursorPath
    [System.IO.File]::WriteAllText($cursorPath, $lastCreatedAt.ToString(), [System.Text.Encoding]::UTF8)
}

Write-Host ("l1l4_auto_payout_out: processed_vouchers=" + $processedVoucherCount + " dispatched=" + $dispatchedCount + " skipped=" + $skippedCount + " dispatch_index=" + $dispatchIndexPath)
