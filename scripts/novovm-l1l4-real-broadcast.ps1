[CmdletBinding()]
param(
    [string]$RepoRoot = "",
    [string]$DispatchIndexFile = "artifacts/l1/l1l4-payout-dispatch.jsonl",
    [string]$AddressMapFile = "artifacts/l1/payout-address-map.json",
    [string]$OutputDir = "artifacts/l1/payout-submitted",
    [string]$SubmittedIndexFile = "artifacts/l1/l1l4-payout-submitted.jsonl",
    [string]$CursorFile = "artifacts/l1/l1l4-payout-submit.cursor",
    [string]$RpcEndpoint = "http://127.0.0.1:9899",
    [ValidateSet("eth_sendTransaction", "eth_sendRawTransaction")]
    [string]$RpcMethod = "eth_sendTransaction",
    [string]$SenderAddress = "",
    [UInt64]$WeiPerRewardUnit = 1,
    [UInt64]$GasLimit = 21000,
    [UInt64]$MaxFeePerGasWei = 0,
    [UInt64]$MaxPriorityFeePerGasWei = 0,
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

function To-HexQty {
    param([UInt64]$Value)
    return ("0x" + $Value.ToString("x"))
}

function Normalize-EvmAddress {
    param([string]$Address)
    if ([string]::IsNullOrWhiteSpace($Address)) {
        return ""
    }
    $v = $Address.Trim()
    if ($v -match '^(0x)?[0-9a-fA-F]{40}$') {
        if ($v.StartsWith("0x") -or $v.StartsWith("0X")) {
            return $v.ToLower()
        }
        return ("0x" + $v.ToLower())
    }
    return ""
}

function Build-AddressMapTable {
    param([string]$PathValue)
    $table = @{}
    if (-not (Test-Path -LiteralPath $PathValue)) {
        return $table
    }
    $raw = Get-Content -LiteralPath $PathValue -Raw
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $table
    }
    $obj = $raw | ConvertFrom-Json -ErrorAction Stop
    if ($obj -is [System.Collections.IDictionary]) {
        foreach ($k in $obj.Keys) {
            $table[[string]$k] = [string]$obj[$k]
        }
        return $table
    }
    $props = $obj.PSObject.Properties
    foreach ($p in $props) {
        $table[[string]$p.Name] = [string]$p.Value
    }
    return $table
}

function Resolve-PayoutAddress {
    param(
        [string]$NodeId,
        [string]$PayoutAccount,
        [hashtable]$AddressMap
    )
    $direct = Normalize-EvmAddress -Address $PayoutAccount
    if ($direct) {
        return $direct
    }
    if ($AddressMap.ContainsKey($PayoutAccount)) {
        $mapped = Normalize-EvmAddress -Address ([string]$AddressMap[$PayoutAccount])
        if ($mapped) {
            return $mapped
        }
    }
    if ($AddressMap.ContainsKey($NodeId)) {
        $mapped = Normalize-EvmAddress -Address ([string]$AddressMap[$NodeId])
        if ($mapped) {
            return $mapped
        }
    }
    return ""
}

function Invoke-Rpc {
    param(
        [string]$Endpoint,
        [string]$Method,
        [object[]]$Params,
        [int]$TimeoutSec
    )
    $reqObj = [ordered]@{
        jsonrpc = "2.0"
        id = 1
        method = $Method
        params = $Params
    }
    $body = $reqObj | ConvertTo-Json -Compress -Depth 8
    return Invoke-RestMethod -Method Post -Uri $Endpoint -ContentType "application/json" -Body $body -TimeoutSec $TimeoutSec
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$dispatchIndexPath = Resolve-FullPath -Root $RepoRoot -Value $DispatchIndexFile
$addressMapPath = Resolve-FullPath -Root $RepoRoot -Value $AddressMapFile
$outDir = Resolve-FullPath -Root $RepoRoot -Value $OutputDir
$submittedIndexPath = Resolve-FullPath -Root $RepoRoot -Value $SubmittedIndexFile
$cursorPath = Resolve-FullPath -Root $RepoRoot -Value $CursorFile

if (-not (Test-Path -LiteralPath $dispatchIndexPath)) {
    throw ("dispatch index file not found: " + $dispatchIndexPath)
}

$fromAddress = Normalize-EvmAddress -Address $SenderAddress
if ($RpcMethod -eq "eth_sendTransaction" -and [string]::IsNullOrWhiteSpace($fromAddress)) {
    throw "SenderAddress is required for eth_sendTransaction and must be a valid EVM address"
}

$cursorDispatchAt = [UInt64]0
if (-not $FullReplay -and (Test-Path -LiteralPath $cursorPath)) {
    $rawCursor = (Get-Content -LiteralPath $cursorPath -Raw).Trim()
    if ($rawCursor) {
        try {
            $cursorDispatchAt = [UInt64]$rawCursor
        } catch {
            throw ("invalid submit cursor at " + $cursorPath + ": " + $rawCursor)
        }
    }
}

$addressMap = Build-AddressMapTable -PathValue $addressMapPath
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Ensure-ParentDir -PathValue $submittedIndexPath

$processedDispatches = 0
$submittedCount = 0
$errorCount = 0
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
        $weiAmount = [UInt64]($rewardUnits * $WeiPerRewardUnit)
        $nowMs = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $status = "submitted_v1"
        $submitErr = $null
        $txHash = $null
        $toAddress = Resolve-PayoutAddress -NodeId $nodeId -PayoutAccount $payoutAccount -AddressMap $addressMap

        if ($RpcMethod -eq "eth_sendTransaction") {
            if ([string]::IsNullOrWhiteSpace($toAddress)) {
                $status = "submit_error_v1"
                $submitErr = "target address unresolved; provide payout-address-map or direct 0x payout_account"
            } else {
                $tx = [ordered]@{
                    from = $fromAddress
                    to = $toAddress
                    value = (To-HexQty -Value $weiAmount)
                }
                if ($GasLimit -gt 0) {
                    $tx["gas"] = (To-HexQty -Value $GasLimit)
                }
                if ($MaxFeePerGasWei -gt 0) {
                    $tx["maxFeePerGas"] = (To-HexQty -Value $MaxFeePerGasWei)
                }
                if ($MaxPriorityFeePerGasWei -gt 0) {
                    $tx["maxPriorityFeePerGas"] = (To-HexQty -Value $MaxPriorityFeePerGasWei)
                }
                try {
                    $resp = Invoke-Rpc -Endpoint $RpcEndpoint -Method $RpcMethod -Params @($tx) -TimeoutSec $RpcTimeoutSec
                    if ($null -ne $resp.result -and -not [string]::IsNullOrWhiteSpace([string]$resp.result)) {
                        $txHash = [string]$resp.result
                    } else {
                        $status = "submit_error_v1"
                        if ($null -ne $resp.error) {
                            $submitErr = ($resp.error | ConvertTo-Json -Compress -Depth 4)
                        } else {
                            $submitErr = "empty rpc result"
                        }
                    }
                } catch {
                    $status = "submit_error_v1"
                    $submitErr = $_.Exception.Message
                }
            }
        } else {
            $rawTx = [string]$inst.signed_raw_tx_hex
            if ([string]::IsNullOrWhiteSpace($rawTx)) {
                $status = "submit_error_v1"
                $submitErr = "missing signed_raw_tx_hex for eth_sendRawTransaction"
            } else {
                try {
                    $resp = Invoke-Rpc -Endpoint $RpcEndpoint -Method $RpcMethod -Params @($rawTx) -TimeoutSec $RpcTimeoutSec
                    if ($null -ne $resp.result -and -not [string]::IsNullOrWhiteSpace([string]$resp.result)) {
                        $txHash = [string]$resp.result
                    } else {
                        $status = "submit_error_v1"
                        if ($null -ne $resp.error) {
                            $submitErr = ($resp.error | ConvertTo-Json -Compress -Depth 4)
                        } else {
                            $submitErr = "empty rpc result"
                        }
                    }
                } catch {
                    $status = "submit_error_v1"
                    $submitErr = $_.Exception.Message
                }
            }
        }

        if ($status -eq "submitted_v1") {
            $submittedCount = $submittedCount + 1
        } else {
            $errorCount = $errorCount + 1
        }

        $rows += [pscustomobject][ordered]@{
            version = 1
            voucher_id = $voucherId
            payout_id = $payoutId
            node_id = $nodeId
            payout_account = $payoutAccount
            to_address = $toAddress
            reward_units = $rewardUnits
            wei_amount = $weiAmount
            submit_method = $RpcMethod
            submit_endpoint = $RpcEndpoint
            tx_hash_hex = $txHash
            status = $status
            error = $submitErr
            submitted_at_unix_ms = $nowMs
        }
    }

    $submittedVoucher = [ordered]@{
        version = 1
        voucher_id = $voucherId
        dispatch_created_at_unix_ms = $dispatchAt
        submitted_created_at_unix_ms = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        submit_method = $RpcMethod
        submit_endpoint = $RpcEndpoint
        submitted_count = [UInt64](@($rows | Where-Object { $_.status -eq "submitted_v1" }).Count)
        error_count = [UInt64](@($rows | Where-Object { $_.status -ne "submitted_v1" }).Count)
        payout_submissions = $rows
    }

    $voucherOutPath = Join-Path $outDir ($voucherId + ".submitted.json")
    [System.IO.File]::WriteAllText(
        $voucherOutPath,
        ($submittedVoucher | ConvertTo-Json -Depth 8),
        [System.Text.Encoding]::UTF8
    )

    $submitLine = ($submittedVoucher | ConvertTo-Json -Depth 8 -Compress) + "`n"
    [System.IO.File]::AppendAllText($submittedIndexPath, $submitLine, [System.Text.Encoding]::UTF8)

    $processedDispatches = $processedDispatches + 1
    if ($dispatchAt -gt $lastDispatchAt) {
        $lastDispatchAt = $dispatchAt
    }
}

if ($processedDispatches -eq 0) {
    Write-Host ("l1l4_real_broadcast_out: no_new_dispatch cursor_dispatch_at=" + $cursorDispatchAt)
    exit 0
}

if (-not $NoCursorUpdate) {
    Ensure-ParentDir -PathValue $cursorPath
    [System.IO.File]::WriteAllText($cursorPath, $lastDispatchAt.ToString(), [System.Text.Encoding]::UTF8)
}

Write-Host ("l1l4_real_broadcast_out: processed_dispatches=" + $processedDispatches + " submitted=" + $submittedCount + " errors=" + $errorCount + " submitted_index=" + $submittedIndexPath)
