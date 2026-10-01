[CmdletBinding()]
param(
    [string]$RepoRoot = "",
    [string]$DispatchIndexFile = "artifacts/l1/l1l4-payout-dispatch.jsonl",
    [string]$SubmittedIndexFile = "artifacts/l1/l1l4-payout-submitted.jsonl",
    [string]$AddressMapFile = "artifacts/l1/payout-address-map.json",
    [string]$OutputDir = "artifacts/l1/payout-reconcile",
    [string]$ReconcileIndexFile = "artifacts/l1/l1l4-payout-reconcile.jsonl",
    [string]$StateFile = "artifacts/l1/l1l4-payout-state.json",
    [string]$CursorFile = "artifacts/l1/l1l4-payout-reconcile.cursor",
    [string]$RpcEndpoint = "http://127.0.0.1:9899",
    [string]$ConfirmMethod = "eth_getTransactionReceipt",
    [string]$SubmitMethod = "eth_sendTransaction",
    [string]$SenderAddress = "",
    [UInt64]$WeiPerRewardUnit = 1,
    [UInt64]$GasLimit = 21000,
    [UInt64]$MaxFeePerGasWei = 0,
    [UInt64]$MaxPriorityFeePerGasWei = 0,
    [int]$RpcTimeoutSec = 15,
    [ValidateRange(0, 1000)]
    [int]$ReplayMaxPerPayout = 3,
    [ValidateRange(0, 86400)]
    [int]$ReplayCooldownSec = 30,
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
    } else {
        foreach ($p in $obj.PSObject.Properties) {
            $table[[string]$p.Name] = [string]$p.Value
        }
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

function New-StateEntry {
    param(
        [string]$PayoutId,
        [string]$VoucherId,
        [string]$NodeId,
        [string]$PayoutAccount,
        [UInt64]$RewardUnits
    )
    return [ordered]@{
        version = 1
        payout_id = $PayoutId
        voucher_id = $VoucherId
        node_id = $NodeId
        payout_account = $PayoutAccount
        reward_units = $RewardUnits
        status = "new_v1"
        tx_hash_hex = $null
        confirm_block_number_hex = $null
        submit_count = [UInt64]0
        replay_count = [UInt64]0
        last_submit_at_unix_ms = [UInt64]0
        last_confirm_at_unix_ms = [UInt64]0
        last_error = $null
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$dispatchIndexPath = Resolve-FullPath -Root $RepoRoot -Value $DispatchIndexFile
$submittedIndexPath = Resolve-FullPath -Root $RepoRoot -Value $SubmittedIndexFile
$addressMapPath = Resolve-FullPath -Root $RepoRoot -Value $AddressMapFile
$outDir = Resolve-FullPath -Root $RepoRoot -Value $OutputDir
$reconcileIndexPath = Resolve-FullPath -Root $RepoRoot -Value $ReconcileIndexFile
$statePath = Resolve-FullPath -Root $RepoRoot -Value $StateFile
$cursorPath = Resolve-FullPath -Root $RepoRoot -Value $CursorFile

if (-not (Test-Path -LiteralPath $dispatchIndexPath)) {
    throw ("dispatch index file not found: " + $dispatchIndexPath)
}
if (-not (Test-Path -LiteralPath $submittedIndexPath)) {
    throw ("submitted index file not found: " + $submittedIndexPath)
}

$fromAddress = Normalize-EvmAddress -Address $SenderAddress
if ([string]::IsNullOrWhiteSpace($fromAddress)) {
    throw "SenderAddress is required and must be a valid EVM address"
}

$cursorDispatchAt = [UInt64]0
if (-not $FullReplay -and (Test-Path -LiteralPath $cursorPath)) {
    $rawCursor = (Get-Content -LiteralPath $cursorPath -Raw).Trim()
    if ($rawCursor) {
        try {
            $cursorDispatchAt = [UInt64]$rawCursor
        } catch {
            throw ("invalid reconcile cursor at " + $cursorPath + ": " + $rawCursor)
        }
    }
}

$addressMap = Build-AddressMapTable -PathValue $addressMapPath

$dispatchMap = @{}
$lineNo = 0
foreach ($line in Get-Content -LiteralPath $dispatchIndexPath) {
    $lineNo = $lineNo + 1
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $obj = $null
    try {
        $obj = $line | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw ("invalid dispatch json at line " + $lineNo + " in " + $dispatchIndexPath + ": " + $_.Exception.Message)
    }
    $voucherId = [string]$obj.voucher_id
    $rows = @($obj.payout_instructions)
    foreach ($r in $rows) {
        $pid = [string]$r.payout_id
        if ([string]::IsNullOrWhiteSpace($pid)) {
            continue
        }
        $dispatchMap[$pid] = [ordered]@{
            payout_id = $pid
            voucher_id = $voucherId
            node_id = [string]$r.node_id
            payout_account = [string]$r.payout_account
            reward_units = To-U64 $r.reward_units
        }
    }
}

$state = [ordered]@{
    version = 1
    updated_at_unix_ms = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    payouts = @{}
}
if (Test-Path -LiteralPath $statePath) {
    $raw = Get-Content -LiteralPath $statePath -Raw
    if (-not [string]::IsNullOrWhiteSpace($raw)) {
        $tmp = $raw | ConvertFrom-Json -ErrorAction Stop
        if ($tmp -and $tmp.payouts) {
            $state = [ordered]@{
                version = 1
                updated_at_unix_ms = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                payouts = @{}
            }
            foreach ($p in $tmp.payouts.PSObject.Properties) {
                $state.payouts[[string]$p.Name] = $p.Value
            }
        }
    }
}

$submittedSeen = 0
$lastDispatchAt = $cursorDispatchAt
$submitRows = @()
$lineNo = 0
foreach ($line in Get-Content -LiteralPath $submittedIndexPath) {
    $lineNo = $lineNo + 1
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $obj = $null
    try {
        $obj = $line | ConvertFrom-Json -ErrorAction Stop
    } catch {
        throw ("invalid submitted json at line " + $lineNo + " in " + $submittedIndexPath + ": " + $_.Exception.Message)
    }
    $dispatchAt = To-U64 $obj.dispatch_created_at_unix_ms
    if (-not $FullReplay -and $dispatchAt -le $cursorDispatchAt) {
        continue
    }
    $rows = @($obj.payout_submissions)
    foreach ($r in $rows) {
        $submitRows += $r
    }
    $submittedSeen = $submittedSeen + 1
    if ($dispatchAt -gt $lastDispatchAt) {
        $lastDispatchAt = $dispatchAt
    }
}

foreach ($r in $submitRows) {
    $pid = [string]$r.payout_id
    if ([string]::IsNullOrWhiteSpace($pid)) {
        continue
    }
    $voucherId = [string]$r.voucher_id
    $nodeId = [string]$r.node_id
    $payoutAccount = [string]$r.payout_account
    $rewardUnits = To-U64 $r.reward_units
    if ($dispatchMap.ContainsKey($pid)) {
        $src = $dispatchMap[$pid]
        if ([string]::IsNullOrWhiteSpace($voucherId)) { $voucherId = [string]$src.voucher_id }
        if ([string]::IsNullOrWhiteSpace($nodeId)) { $nodeId = [string]$src.node_id }
        if ([string]::IsNullOrWhiteSpace($payoutAccount)) { $payoutAccount = [string]$src.payout_account }
        if ($rewardUnits -eq 0) { $rewardUnits = To-U64 $src.reward_units }
    }
    if (-not $state.payouts.ContainsKey($pid)) {
        $state.payouts[$pid] = New-StateEntry -PayoutId $pid -VoucherId $voucherId -NodeId $nodeId -PayoutAccount $payoutAccount -RewardUnits $rewardUnits
    }
    $entry = $state.payouts[$pid]
    $entry.status = [string]$r.status
    $entry.tx_hash_hex = [string]$r.tx_hash_hex
    $entry.last_submit_at_unix_ms = To-U64 $r.submitted_at_unix_ms
    $entry.last_error = [string]$r.error
    $entry.submit_count = To-U64 $entry.submit_count
    if ($entry.status -eq "submitted_v1") {
        $entry.submit_count = [UInt64]($entry.submit_count + 1)
    }
    $state.payouts[$pid] = $entry
}

$nowMs = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$cooldownMs = [UInt64]($ReplayCooldownSec * 1000)

$confirmedCount = 0
$replayedCount = 0
$pendingCount = 0
$errorCount = 0
$changed = @()

$keys = @($state.payouts.Keys)
foreach ($pid in $keys) {
    $entry = $state.payouts[$pid]
    $status = [string]$entry.status
    $txHash = Normalize-TxHash -TxHashHex ([string]$entry.tx_hash_hex)

    if ($status -eq "confirmed_v1") {
        continue
    }

    $confirmed = $false
    if (-not [string]::IsNullOrWhiteSpace($txHash)) {
        try {
            $resp = Invoke-Rpc -Endpoint $RpcEndpoint -Method $ConfirmMethod -Params @($txHash) -TimeoutSec $RpcTimeoutSec
            if ($null -ne $resp.result) {
                $entry.status = "confirmed_v1"
                $entry.confirm_block_number_hex = [string]$resp.result.blockNumber
                $entry.last_confirm_at_unix_ms = $nowMs
                $entry.tx_hash_hex = if ($null -ne $resp.result.transactionHash -and -not [string]::IsNullOrWhiteSpace([string]$resp.result.transactionHash)) { [string]$resp.result.transactionHash } else { $txHash }
                $entry.last_error = $null
                $confirmed = $true
                $confirmedCount = $confirmedCount + 1
                $changed += [pscustomobject][ordered]@{
                    payout_id = $pid
                    action = "confirm"
                    tx_hash_hex = $entry.tx_hash_hex
                    status = $entry.status
                }
            }
        } catch {
            $entry.last_error = $_.Exception.Message
            $errorCount = $errorCount + 1
        }
    }

    if ($confirmed) {
        $state.payouts[$pid] = $entry
        continue
    }

    $entry.replay_count = To-U64 $entry.replay_count
    $lastSubmitAt = To-U64 $entry.last_submit_at_unix_ms
    $due = ($cooldownMs -eq 0) -or ($lastSubmitAt -eq 0) -or (($nowMs - $lastSubmitAt) -ge $cooldownMs)
    $canReplay = ($entry.replay_count -lt [UInt64]$ReplayMaxPerPayout) -and $due

    if ($canReplay) {
        $toAddr = Resolve-PayoutAddress -NodeId ([string]$entry.node_id) -PayoutAccount ([string]$entry.payout_account) -AddressMap $addressMap
        if ([string]::IsNullOrWhiteSpace($toAddr)) {
            $entry.status = "replay_error_v1"
            $entry.last_error = "target address unresolved in replay"
            $errorCount = $errorCount + 1
        } else {
            $weiAmount = [UInt64]((To-U64 $entry.reward_units) * $WeiPerRewardUnit)
            $tx = [ordered]@{
                from = $fromAddress
                to = $toAddr
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
                $resp = Invoke-Rpc -Endpoint $RpcEndpoint -Method $SubmitMethod -Params @($tx) -TimeoutSec $RpcTimeoutSec
                if ($null -ne $resp.result -and -not [string]::IsNullOrWhiteSpace([string]$resp.result)) {
                    $entry.tx_hash_hex = [string]$resp.result
                    $entry.status = "submitted_v1"
                    $entry.last_submit_at_unix_ms = $nowMs
                    $entry.submit_count = [UInt64]((To-U64 $entry.submit_count) + 1)
                    $entry.replay_count = [UInt64]($entry.replay_count + 1)
                    $entry.last_error = $null
                    $replayedCount = $replayedCount + 1
                    $changed += [pscustomobject][ordered]@{
                        payout_id = $pid
                        action = "replay_submit"
                        tx_hash_hex = $entry.tx_hash_hex
                        status = $entry.status
                    }
                } else {
                    $entry.status = "replay_error_v1"
                    if ($null -ne $resp.error) {
                        $entry.last_error = ($resp.error | ConvertTo-Json -Compress -Depth 4)
                    } else {
                        $entry.last_error = "empty replay rpc result"
                    }
                    $entry.replay_count = [UInt64]($entry.replay_count + 1)
                    $errorCount = $errorCount + 1
                }
            } catch {
                $entry.status = "replay_error_v1"
                $entry.last_error = $_.Exception.Message
                $entry.replay_count = [UInt64]($entry.replay_count + 1)
                $errorCount = $errorCount + 1
            }
        }
    } else {
        $pendingCount = $pendingCount + 1
    }

    $state.payouts[$pid] = $entry
}

$state.updated_at_unix_ms = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Ensure-ParentDir -PathValue $statePath
Ensure-ParentDir -PathValue $reconcileIndexPath

[System.IO.File]::WriteAllText($statePath, ($state | ConvertTo-Json -Depth 8), [System.Text.Encoding]::UTF8)

$reconcile = [ordered]@{
    version = 1
    created_at_unix_ms = [UInt64][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    rpc_endpoint = $RpcEndpoint
    confirm_method = $ConfirmMethod
    submit_method = $SubmitMethod
    processed_submitted_records = [UInt64]$submittedSeen
    payout_state_size = [UInt64](@($state.payouts.Keys).Count)
    confirmed_count = [UInt64]$confirmedCount
    replayed_count = [UInt64]$replayedCount
    pending_count = [UInt64]$pendingCount
    error_count = [UInt64]$errorCount
    changed = $changed
}

$snapshotName = ("reconcile-{0}.json" -f ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()))
$snapshotPath = Join-Path $outDir $snapshotName
[System.IO.File]::WriteAllText($snapshotPath, ($reconcile | ConvertTo-Json -Depth 8), [System.Text.Encoding]::UTF8)

$line = ($reconcile | ConvertTo-Json -Depth 8 -Compress) + "`n"
[System.IO.File]::AppendAllText($reconcileIndexPath, $line, [System.Text.Encoding]::UTF8)

if (-not $NoCursorUpdate) {
    Ensure-ParentDir -PathValue $cursorPath
    [System.IO.File]::WriteAllText($cursorPath, $lastDispatchAt.ToString(), [System.Text.Encoding]::UTF8)
}

Write-Host ("l1l4_reconcile_out: processed_submitted=" + $submittedSeen + " state_size=" + (@($state.payouts.Keys).Count) + " confirmed=" + $confirmedCount + " replayed=" + $replayedCount + " pending=" + $pendingCount + " errors=" + $errorCount + " snapshot=" + $snapshotPath)
