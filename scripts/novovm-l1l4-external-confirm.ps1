[CmdletBinding()]
param(
    [string]$RepoRoot = "",
    [string]$ExecutedIndexFile = "artifacts/l1/l1l4-payout-executed.jsonl",
    [string]$OutputDir = "artifacts/l1/payout-confirmed",
    [string]$ConfirmIndexFile = "artifacts/l1/l1l4-payout-confirmed.jsonl",
    [string]$CursorFile = "artifacts/l1/l1l4-payout-confirm.cursor",
    [string]$RpcEndpoint = "http://127.0.0.1:9899",
    [string]$RpcMethod = "eth_getTransactionReceipt",
    [int]$RpcTimeoutSec = 15,
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

function Normalize-TxHash {
    param([string]$TxHashHex)
    if ([string]::IsNullOrWhiteSpace($TxHashHex)) {
        return ""
    }
    $v = $TxHashHex.Trim()
    if ($v.StartsWith("0x") -or $v.StartsWith("0X")) {
        return $v.ToLower()
    }
    return ("0x" + $v.ToLower())
}

function Invoke-EthReceiptQuery {
    param(
        [string]$Endpoint,
        [string]$Method,
        [string]$TxHash,
        [int]$TimeoutSec
    )
    $reqObj = [ordered]@{
        jsonrpc = "2.0"
        id = 1
        method = $Method
        params = @($TxHash)
    }
    $body = $reqObj | ConvertTo-Json -Compress -Depth 4
    return Invoke-RestMethod -Method Post -Uri $Endpoint -ContentType "application/json" -Body $body -TimeoutSec $TimeoutSec
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$executedIndexPath = Resolve-FullPath -Root $RepoRoot -Value $ExecutedIndexFile
$outDir = Resolve-FullPath -Root $RepoRoot -Value $OutputDir
$confirmIndexPath = Resolve-FullPath -Root $RepoRoot -Value $ConfirmIndexFile
$cursorPath = Resolve-FullPath -Root $RepoRoot -Value $CursorFile

if (-not (Test-Path -LiteralPath $executedIndexPath)) {
    throw ("executed index file not found: " + $executedIndexPath)
}

$cursorExecutedAt = [UInt64]0
if (-not $FullReplay -and (Test-Path -LiteralPath $cursorPath)) {
    $rawCursor = (Get-Content -LiteralPath $cursorPath -Raw).Trim()
    if ($rawCursor) {
        try {
            $cursorExecutedAt = [UInt64]$rawCursor
        } catch {
            throw ("invalid confirm cursor at " + $cursorPath + ": " + $rawCursor)
        }
    }
}

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Ensure-ParentDir -PathValue $confirmIndexPath

$processedExecutions = 0
$confirmedTotal = 0
$pendingTotal = 0
$errorTotal = 0
$lastExecutedAt = $cursorExecutedAt

$lineNo = 0
foreach ($line in Get-Content -LiteralPath $executedIndexPath) {
    $lineNo = $lineNo + 1
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }

    $executed = $null
    try {
        $executed = $line | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw ("invalid executed json at line " + $lineNo + " in " + $executedIndexPath + ": " + $_.Exception.Message)
    }

    $recordAt = To-U64 $executed.executed_created_at_unix_ms
    $sourceRows = @()
    if ($null -ne $executed.payout_executions) {
        $sourceRows = @($executed.payout_executions)
    } elseif ($null -ne $executed.payout_submissions) {
        $sourceRows = @($executed.payout_submissions)
        $recordAt = To-U64 $executed.submitted_created_at_unix_ms
    } else {
        $sourceRows = @()
    }

    if (-not $FullReplay -and $recordAt -le $cursorExecutedAt) {
        continue
    }

    $voucherId = [string]$executed.voucher_id
    if ([string]::IsNullOrWhiteSpace($voucherId)) {
        throw ("voucher_id missing at line " + $lineNo + " in " + $executedIndexPath)
    }

    $rows = @()
    $confirmCount = 0
    $pendingCount = 0
    $errorCount = 0

    foreach ($row in $sourceRows) {
        $payoutId = [string]$row.payout_id
        $txHashHex = [string]$row.tx_hash_hex
        $txHash = Normalize-TxHash -TxHashHex $txHashHex
        $status = "pending_external_confirm_v1"
        $confirmBlockHex = $null
        $confirmTxHash = $txHash
        $errorMessage = $null

        if ([string]::IsNullOrWhiteSpace($txHash)) {
            $status = "confirm_error_v1"
            $errorMessage = "missing tx_hash_hex"
            $errorCount = $errorCount + 1
        } else {
            try {
                $resp = Invoke-EthReceiptQuery -Endpoint $RpcEndpoint -Method $RpcMethod -TxHash $txHash -TimeoutSec $RpcTimeoutSec
                if ($null -ne $resp -and $null -ne $resp.result) {
                    $status = "confirmed_v1"
                    $confirmCount = $confirmCount + 1
                    if ($null -ne $resp.result.blockNumber) {
                        $confirmBlockHex = [string]$resp.result.blockNumber
                    }
                    if ($null -ne $resp.result.transactionHash -and -not [string]::IsNullOrWhiteSpace([string]$resp.result.transactionHash)) {
                        $confirmTxHash = [string]$resp.result.transactionHash
                    }
                } else {
                    $pendingCount = $pendingCount + 1
                }
            } catch {
                $status = "confirm_error_v1"
                $errorMessage = $_.Exception.Message
                $errorCount = $errorCount + 1
            }
        }

        $rows += [pscustomobject][ordered]@{
            version = 1
            voucher_id = $voucherId
            payout_id = $payoutId
            node_id = [string]$row.node_id
            payout_account = [string]$row.payout_account
            reward_units = To-U64 $row.reward_units
            chain_id = To-U64 $row.chain_id
            tx_hash_hex = $txHashHex
            confirmed_tx_hash = $confirmTxHash
            confirmed_block_number_hex = $confirmBlockHex
            confirm_method = $RpcMethod
            confirm_endpoint = $RpcEndpoint
            status = $status
            error = $errorMessage
            confirmed_at_unix_ms = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        }
    }

    $confirmVoucher = [ordered]@{
        version = 1
        voucher_id = $voucherId
        executed_created_at_unix_ms = $recordAt
        confirmed_created_at_unix_ms = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        confirm_method = $RpcMethod
        confirm_endpoint = $RpcEndpoint
        confirmed_count = [UInt64]$confirmCount
        pending_count = [UInt64]$pendingCount
        error_count = [UInt64]$errorCount
        payout_confirms = $rows
    }

    $voucherOutPath = Join-Path $outDir ($voucherId + ".confirmed.json")
    [System.IO.File]::WriteAllText(
        $voucherOutPath,
        ($confirmVoucher | ConvertTo-Json -Depth 8),
        [System.Text.Encoding]::UTF8
    )

    $confirmLine = ($confirmVoucher | ConvertTo-Json -Depth 8 -Compress) + "`n"
    [System.IO.File]::AppendAllText($confirmIndexPath, $confirmLine, [System.Text.Encoding]::UTF8)

    $processedExecutions = $processedExecutions + 1
    $confirmedTotal = $confirmedTotal + $confirmCount
    $pendingTotal = $pendingTotal + $pendingCount
    $errorTotal = $errorTotal + $errorCount
    if ($recordAt -gt $lastExecutedAt) {
        $lastExecutedAt = $recordAt
    }
}

if ($processedExecutions -eq 0) {
    Write-Host ("l1l4_external_confirm_out: no_new_executed cursor_executed_at=" + $cursorExecutedAt)
    exit 0
}

if (-not $NoCursorUpdate) {
    Ensure-ParentDir -PathValue $cursorPath
    [System.IO.File]::WriteAllText($cursorPath, $lastExecutedAt.ToString(), [System.Text.Encoding]::UTF8)
}

Write-Host ("l1l4_external_confirm_out: processed_executed=" + $processedExecutions + " confirmed=" + $confirmedTotal + " pending=" + $pendingTotal + " errors=" + $errorTotal + " confirm_index=" + $confirmIndexPath)
