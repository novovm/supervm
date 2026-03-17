param(
    [string]$GatewayUrl = "http://127.0.0.1:9899",
    [UInt64]$ChainId = 1,
    [int]$IntervalMs = 1000,
    [int]$SampleMax = 5
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$UniswapV2Router = "0x7a250d5630b4cf539739df2c5dacb4c659f2488d"
$UniswapV3SwapRouter = "0xe592427a0aece92de3edee1f18e0157c05861564"
$UniswapV3SwapRouter02 = "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45"
$UniswapUniversalRouter = "0xef1c6e67703c7bd7107eed8303fbe6ec2554bf6b"

function Invoke-JsonRpc {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params
    )
    $body = @{
        jsonrpc = "2.0"
        id = 1
        method = $Method
        params = $Params
    } | ConvertTo-Json -Depth 32 -Compress
    $resp = Invoke-RestMethod -Uri $GatewayUrl -Method Post -ContentType "application/json" -Body $body -TimeoutSec 20
    if ($resp -is [string]) {
        $resp = $resp | ConvertFrom-Json
    }
    if (($resp.PSObject.Properties.Name -contains "error") -and $null -ne $resp.error) {
        throw ("{0} failed: code={1} message={2}" -f $Method, $resp.error.code, $resp.error.message)
    }
    return $resp.result
}

function Convert-HexToInt {
    param($Value)
    if ($null -eq $Value) {
        return 0
    }
    $raw = [string]$Value
    if ($raw.StartsWith("0x", [System.StringComparison]::OrdinalIgnoreCase)) {
        return [Convert]::ToInt64($raw.Substring(2), 16)
    }
    return [int64]$raw
}

Write-Host ("watching gateway={0} chain_id={1} interval_ms={2}" -f $GatewayUrl, $ChainId, $IntervalMs)
Write-Host ("uniswap_v2={0} uniswap_v3={1}" -f $UniswapV2Router, $UniswapV3SwapRouter)

$prevPending = $null
while ($true) {
    try {
        $peerCountHex = Invoke-JsonRpc -Method "net_peerCount" -Params @()
        $txpoolStatus = Invoke-JsonRpc -Method "txpool_status" -Params @{ chain_id = $ChainId }
        $txpoolContent = Invoke-JsonRpc -Method "txpool_content" -Params @{ chain_id = $ChainId }
        $broadcastStatus = Invoke-JsonRpc -Method "evm_getPublicBroadcastCapability" -Params @{ chain_id = $ChainId }

        $pendingCount = Convert-HexToInt $txpoolStatus.pending
        $queuedCount = Convert-HexToInt $txpoolStatus.queued
        $peerCount = Convert-HexToInt $peerCountHex

        $v2Count = 0
        $v3Count = 0
        $samples = New-Object System.Collections.ArrayList
        $pendingRoot = $txpoolContent.pending
        if ($null -ne $pendingRoot) {
            foreach ($fromProp in $pendingRoot.PSObject.Properties) {
                $from = [string]$fromProp.Name
                $nonceMap = $fromProp.Value
                if ($null -eq $nonceMap) {
                    continue
                }
                foreach ($nonceProp in $nonceMap.PSObject.Properties) {
                    $tx = $nonceProp.Value
                    if ($null -eq $tx) {
                        continue
                    }
                    $to = [string]$tx.to
                    $hash = [string]$tx.hash
                    if ($to.Equals($UniswapV2Router, [System.StringComparison]::OrdinalIgnoreCase)) {
                        $v2Count++
                    }
                    if (
                        $to.Equals($UniswapV3SwapRouter, [System.StringComparison]::OrdinalIgnoreCase) -or
                        $to.Equals($UniswapV3SwapRouter02, [System.StringComparison]::OrdinalIgnoreCase) -or
                        $to.Equals($UniswapUniversalRouter, [System.StringComparison]::OrdinalIgnoreCase)
                    ) {
                        $v3Count++
                    }
                    if ($samples.Count -lt $SampleMax) {
                        [void]$samples.Add([PSCustomObject]@{
                            from  = $from
                            to    = $to
                            nonce = [string]$nonceProp.Name
                            hash  = $hash
                        })
                    }
                }
            }
        }

        $uniTotal = [int64]($v2Count + $v3Count)
        $otherPending = 0
        if ($pendingCount -ge $uniTotal) {
            $otherPending = [int64]($pendingCount - $uniTotal)
        }
        $pendingDelta = if ($null -eq $prevPending) { [int64]0 } else { [int64]($pendingCount - [int64]$prevPending) }
        $prevPending = [int64]$pendingCount
        $uniPct = if ($pendingCount -gt 0) {
            [Math]::Round((100.0 * [double]$uniTotal) / [double]$pendingCount, 2)
        } else {
            [double]0
        }
        $evictedLastTick = Convert-HexToInt $broadcastStatus.native_plugin_mempool_ingest_evicted_last_tick
        $evictedConfirmedLastTick = Convert-HexToInt $broadcastStatus.native_plugin_mempool_ingest_evicted_confirmed_last_tick
        $evictedStaleLastTick = Convert-HexToInt $broadcastStatus.native_plugin_mempool_ingest_evicted_stale_last_tick

        $now = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
        Write-Host ("[{0}] peer={1} pending={2} dPending={3} other={4} queued={5} uniV2={6} uniV3={7} uniTotal={8} uniPct={9}% evict={10}(confirm={11},stale={12})" -f `
            $now, $peerCount, $pendingCount, $pendingDelta, $otherPending, $queuedCount, $v2Count, $v3Count, $uniTotal, $uniPct, `
            $evictedLastTick, $evictedConfirmedLastTick, $evictedStaleLastTick)
        if ($samples.Count -gt 0) {
            $samples | Format-Table from, to, nonce, hash -AutoSize
        } else {
            Write-Host "no pending sample yet"
        }
    } catch {
        Write-Warning ("watch tick failed: {0}" -f $_.Exception.Message)
    }
    Start-Sleep -Milliseconds $IntervalMs
}
