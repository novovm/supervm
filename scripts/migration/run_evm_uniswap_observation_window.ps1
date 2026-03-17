param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [switch]$AttachExistingGateway,
    [UInt64]$ChainId = 1,
    [UInt64]$DurationMinutes = 30,
    [UInt64]$IntervalSeconds = 60,
    [UInt64]$WarmupSeconds = 5,
    [string]$PluginPorts = "30303,30304",
    [switch]$SeedFromCanarySummary,
    [string]$SeedSummaryPath = "artifacts/migration/evm-uniswap-pending-queue-canary-summary.json",
    [UInt64]$SeedMaxImport = 5,
    [UInt64]$SampleMax = 5,
    [string]$SummaryOut = "artifacts/migration/evm-uniswap-observation-window-summary.json",
    [switch]$EnablePluginMempoolIngest,
    [UInt64]$PluginMempoolPollMs = 2000,
    [UInt64]$PluginMinCandidates = 600,
    [UInt64]$RlpxMaxPeersPerTick = 32,
    [string]$RlpxHelloProfile = "geth",
    [string]$RlpxProfilePath = "",
    [switch]$FreshRlpxProfile,
    [switch]$RlpxSingleSession,
    [Nullable[UInt64]]$RlpxCoreTarget = $null,
    [Nullable[UInt64]]$RlpxActiveTarget = $null,
    [Nullable[UInt64]]$RlpxCoreRecentGossipWindowMs = $null,
    [Nullable[UInt64]]$RlpxActiveRecentReadyWindowMs = $null,
    [Nullable[UInt64]]$RlpxCoreLockMs = $null,
    [Nullable[UInt64]]$RlpxRecentNewHashWindowMs = $null,
    [Nullable[UInt64]]$RlpxRecentNewHashMin = $null,
    [Nullable[UInt64]]$RlpxPriorityBudget = $null,
    [Nullable[UInt64]]$RlpxPriorityAutoPoolSize = $null,
    [switch]$EnableSwapPriority,
    [Nullable[UInt64]]$RlpxSwapPriorityLatencyTargetMs = $null,
    [switch]$SmokeAssert,
    [UInt64]$SmokeMinReady = 1,
    [UInt64]$SmokeMinNewPooled = 1,
    [UInt64]$SmokeMinPooled = 1,
    [bool]$SmokeRequireFirstFrameNewPooled = $true,
    [bool]$EnableDnsDiscoverySeed = $true,
    [UInt64]$DnsDiscoveryMaxEnodes = 120,
    [string]$DnsDiscoveryRoot = "all.mainnet.ethdisco.net",
    [string]$FixedPluginEnode = "",
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$UniswapV2Router = "0x7a250d5630b4cf539739df2c5dacb4c659f2488d"
$UniswapV3SwapRouter = "0xe592427a0aece92de3edee1f18e0157c05861564"
$UniswapV3SwapRouter02 = "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45"
$UniswapUniversalRouter = "0xef1c6e67703c7bd7107eed8303fbe6ec2554bf6b"

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

function Resolve-BinaryPath {
    param(
        [string]$TargetRoot,
        [string]$BinaryBaseName
    )
    $isWindowsOs = $env:OS -eq "Windows_NT"
    $exeName = if ($isWindowsOs) { "$BinaryBaseName.exe" } else { $BinaryBaseName }
    return (Join-Path $TargetRoot "debug\$exeName")
}

function Resolve-DnsDiscoveryEnodes {
    param(
        [string]$Root,
        [UInt64]$MaxEnodes,
        [string]$DnsRoot
    )
    $resolverPath = Resolve-FullPath -Root $Root -Value "scripts/migration/resolve_eth_dns_enodes.py"
    if (-not (Test-Path $resolverPath)) {
        return [ordered]@{
            enodes = @()
            error = ("dns resolver script missing: {0}" -f $resolverPath)
        }
    }
    $pythonCandidates = @()
    if (Get-Command python -ErrorAction SilentlyContinue) {
        $pythonCandidates += [pscustomobject]@{
            exe = "python"
            args = @()
        }
    }
    if (Get-Command py -ErrorAction SilentlyContinue) {
        $pythonCandidates += [pscustomobject]@{
            exe = "py"
            args = @("-3")
        }
    }
    if ($pythonCandidates.Count -eq 0) {
        return [ordered]@{
            enodes = @()
            error = "python runtime not found"
        }
    }
    $maxArg = ([int][Math]::Max(1, $MaxEnodes))
    $attemptErrors = New-Object System.Collections.Generic.List[string]
    foreach ($candidate in $pythonCandidates) {
        $pythonExe = [string]$candidate.exe
        $pythonPrefixArgs = @($candidate.args)
        $savedErrorActionPreference = $ErrorActionPreference
        $savedNativeErrorPreference = $null
        $nativePrefExists = $false
        try {
            $ErrorActionPreference = "Continue"
            if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
                $nativePrefExists = $true
                $savedNativeErrorPreference = $PSNativeCommandUseErrorActionPreference
                $PSNativeCommandUseErrorActionPreference = $false
            }
            $raw = (& $pythonExe @pythonPrefixArgs $resolverPath --root $DnsRoot --max-enodes $maxArg --json 2>$null) -join "`n"
            $parsed = $null
            if (-not [string]::IsNullOrWhiteSpace($raw)) {
                try {
                    $jsonStart = $raw.IndexOf('{')
                    $jsonEnd = $raw.LastIndexOf('}')
                    if ($jsonStart -ge 0 -and $jsonEnd -gt $jsonStart) {
                        $raw = $raw.Substring($jsonStart, $jsonEnd - $jsonStart + 1)
                    }
                    $parsed = $raw | ConvertFrom-Json
                } catch {
                    $parsed = $null
                }
            }
            $enodes = @()
            if ($null -ne $parsed -and $null -ne $parsed.enodes) {
                foreach ($entry in @($parsed.enodes)) {
                    $item = [string]$entry
                    if ([string]::IsNullOrWhiteSpace($item)) {
                        continue
                    }
                    $enodes += $item.Trim()
                }
            }
            if ($enodes.Count -eq 0) {
                $fallbackLines = (& $pythonExe @pythonPrefixArgs $resolverPath --root $DnsRoot --max-enodes $maxArg 2>$null) -join "`n"
                if (-not [string]::IsNullOrWhiteSpace($fallbackLines)) {
                    foreach ($line in ($fallbackLines -split "`r?`n")) {
                        $entry = [string]$line
                        if ([string]::IsNullOrWhiteSpace($entry)) {
                            continue
                        }
                        $trimmed = $entry.Trim()
                        if ($trimmed.StartsWith("enode://", [System.StringComparison]::OrdinalIgnoreCase)) {
                            $enodes += $trimmed
                        }
                    }
                }
            }
            if ($enodes.Count -gt 0) {
                return [ordered]@{
                    enodes = $enodes
                    error = $null
                }
            }
            $rawPreview = [string]$raw
            if ($rawPreview.Length -gt 240) {
                $rawPreview = $rawPreview.Substring(0, 240)
            }
            if ([string]::IsNullOrWhiteSpace($rawPreview)) {
                $rawPreview = "dns resolver returned empty payload"
            }
            [void]$attemptErrors.Add(("{0}: {1}" -f $pythonExe, $rawPreview))
        } catch {
            [void]$attemptErrors.Add(("{0}: {1}" -f $pythonExe, $_.Exception.Message))
        } finally {
            $ErrorActionPreference = $savedErrorActionPreference
            if ($nativePrefExists) {
                $PSNativeCommandUseErrorActionPreference = $savedNativeErrorPreference
            }
        }
    }
    if ($attemptErrors.Count -eq 0) {
        return [ordered]@{
            enodes = @()
            error = "dns resolver failed"
        }
    }
    return [ordered]@{
        enodes = @()
        error = ($attemptErrors -join " | ")
    }
}

function Push-ProcessEnv {
    param([hashtable]$Environment)
    $state = @{}
    if ($null -eq $Environment) {
        return $state
    }
    foreach ($key in $Environment.Keys) {
        $envPath = "Env:$key"
        $exists = Test-Path $envPath
        $oldValue = $null
        if ($exists) {
            $oldValue = (Get-Item -Path $envPath).Value
        }
        $state[$key] = [pscustomobject]@{
            exists = $exists
            value = $oldValue
        }
        Set-Item -Path $envPath -Value ([string]$Environment[$key])
    }
    return $state
}

function Pop-ProcessEnv {
    param([hashtable]$State)
    if ($null -eq $State) {
        return
    }
    foreach ($key in $State.Keys) {
        $entry = $State[$key]
        $envPath = "Env:$key"
        if ($entry.exists) {
            Set-Item -Path $envPath -Value ([string]$entry.value)
        } else {
            Remove-Item -Path $envPath -ErrorAction SilentlyContinue
        }
    }
}

function Set-ChainScopedEnvValue {
    param(
        [Parameter(Mandatory = $true)][hashtable]$Environment,
        [Parameter(Mandatory = $true)][string]$BaseKey,
        [Parameter(Mandatory = $true)][UInt64]$ChainId,
        [Parameter(Mandatory = $true)][string]$ChainHex,
        [Parameter(Mandatory = $true)][string]$Value
    )
    $Environment[$BaseKey] = $Value
    $Environment[("{0}_CHAIN_{1}" -f $BaseKey, $ChainId)] = $Value
    $Environment[("{0}_CHAIN_{1}" -f $BaseKey, $ChainHex)] = $Value
}

function Join-BoundedCsv {
    param(
        [string[]]$Items,
        [int]$MaxLength = 28000
    )
    $selected = New-Object System.Collections.Generic.List[string]
    $currentLength = 0
    $skipped = 0
    if ($null -eq $Items) {
        return [ordered]@{
            csv = ""
            included = 0
            skipped = 0
            length = 0
        }
    }
    foreach ($raw in $Items) {
        $entry = [string]$raw
        if ([string]::IsNullOrWhiteSpace($entry)) {
            continue
        }
        $trimmed = $entry.Trim()
        $delta = $trimmed.Length
        if ($selected.Count -gt 0) {
            $delta = $delta + 1 # comma
        }
        if (($currentLength + $delta) -gt $MaxLength) {
            $skipped++
            continue
        }
        $selected.Add($trimmed)
        $currentLength = $currentLength + $delta
    }
    return [ordered]@{
        csv = ($selected -join ",")
        included = [int]$selected.Count
        skipped = [int]$skipped
        length = [int]$currentLength
    }
}

function Invoke-JsonRpc {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params,
        [UInt64]$TimeoutSec = 20
    )
    $body = @{
        jsonrpc = "2.0"
        id = 1
        method = $Method
        params = $Params
    } | ConvertTo-Json -Depth 64 -Compress
    $resp = Invoke-RestMethod -Uri $Url -Method Post -ContentType "application/json" -Body $body -TimeoutSec ([int]$TimeoutSec)
    if ($resp -is [string]) {
        $resp = $resp | ConvertFrom-Json
    }
    if (($resp.PSObject.Properties.Name -contains "error") -and $null -ne $resp.error) {
        throw ("{0} failed: code={1} message={2}" -f $Method, $resp.error.code, $resp.error.message)
    }
    if (-not ($resp.PSObject.Properties.Name -contains "result")) {
        throw ("{0} failed: response missing result" -f $Method)
    }
    return $resp.result
}

function Convert-HexToUInt64 {
    param($Value)
    if ($null -eq $Value) {
        return [UInt64]0
    }
    if ($Value -is [UInt64]) {
        return [UInt64]$Value
    }
    $raw = ([string]$Value).Trim()
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return [UInt64]0
    }
    if ($raw.StartsWith("0x", [System.StringComparison]::OrdinalIgnoreCase)) {
        $hex = $raw.Substring(2)
        if ([string]::IsNullOrWhiteSpace($hex)) {
            return [UInt64]0
        }
        return [Convert]::ToUInt64($hex, 16)
    }
    return [UInt64]$raw
}

function Import-SeedCandidates {
    param(
        [string]$GatewayUrl,
        [UInt64]$ChainId,
        [string]$SummaryPath,
        [UInt64]$SeedMax
    )
    $result = [ordered]@{
        requested = [int]$SeedMax
        attempted = 0
        imported = 0
        failed = 0
        errors = @()
    }
    if (-not (Test-Path $SummaryPath)) {
        $result.errors += ("seed summary not found: {0}" -f $SummaryPath)
        return $result
    }
    $summary = Get-Content -Raw $SummaryPath | ConvertFrom-Json
    if ($null -eq $summary.source_candidates) {
        $result.errors += "seed summary missing source_candidates"
        return $result
    }
    $candidates = @($summary.source_candidates) | Select-Object -First ([int][Math]::Max(1, $SeedMax))
    foreach ($cand in $candidates) {
        $result.attempted++
        try {
            $from = [string]$cand.from
            $rawTx = [string]$cand.raw_tx
            if ([string]::IsNullOrWhiteSpace($from) -or [string]::IsNullOrWhiteSpace($rawTx)) {
                throw "candidate missing from/raw_tx"
            }
            $seedTag = "{0}-{1}" -f $result.attempted, ([Guid]::NewGuid().ToString("N").Substring(0, 8))
            $ucaId = ("uca-watch-uniswap-{0}" -f $seedTag)
            try {
                [void](Invoke-JsonRpc -Url $GatewayUrl -Method "ua_createUca" -Params @{ uca_id = $ucaId })
            } catch {
            }

            try {
                [void](Invoke-JsonRpc -Url $GatewayUrl -Method "ua_bindPersona" -Params @{
                    uca_id = $ucaId
                    persona_type = "evm"
                    chain_id = [UInt64]$ChainId
                    external_address = $from
                })
            } catch {
                $bindMsg = $_.Exception.Message
                $ownerMatch = [regex]::Match($bindMsg, "existing owner:\s*([a-zA-Z0-9_\-]+)")
                if ($ownerMatch.Success) {
                    $ucaId = $ownerMatch.Groups[1].Value
                } else {
                    throw
                }
            }

            $sendParams = @{
                uca_id = $ucaId
                chain_id = [UInt64]$ChainId
                from = $from
                raw_tx = $rawTx
                require_public_broadcast = $false
                return_detail = $false
            }
            try {
                [void](Invoke-JsonRpc -Url $GatewayUrl -Method "eth_sendRawTransaction" -Params $sendParams)
            } catch {
                $sendMsg = $_.Exception.Message
                $ownerMatch = [regex]::Match($sendMsg, "binding_owner=([a-zA-Z0-9_\-]+)")
                if (-not $ownerMatch.Success) {
                    throw
                }
                $sendParams.uca_id = $ownerMatch.Groups[1].Value
                [void](Invoke-JsonRpc -Url $GatewayUrl -Method "eth_sendRawTransaction" -Params $sendParams)
            }
            $result.imported++
        } catch {
            $result.failed++
            $result.errors += $_.Exception.Message
        }
    }
    return $result
}

function Measure-UniswapState {
    param(
        [string]$GatewayUrl,
        [UInt64]$ChainId,
        [UInt64]$SampleMax
    )
    $peerCountHex = Invoke-JsonRpc -Url $GatewayUrl -Method "net_peerCount" -Params @{ chain_id = [UInt64]$ChainId }
    $txpoolStatus = Invoke-JsonRpc -Url $GatewayUrl -Method "txpool_status" -Params @{ chain_id = [UInt64]$ChainId }
    $txpoolContent = Invoke-JsonRpc -Url $GatewayUrl -Method "txpool_content" -Params @{ chain_id = [UInt64]$ChainId }
    $pendingIngress = Invoke-JsonRpc -Url $GatewayUrl -Method "evm_snapshotPendingIngress" -Params @{
        chain_id = [UInt64]$ChainId
        max_items = 2048
        include_raw = $false
        include_parsed = $false
    }
    $broadcastStatus = Invoke-JsonRpc -Url $GatewayUrl -Method "evm_getPublicBroadcastCapability" -Params @{
        chain_id = [UInt64]$ChainId
    }
    $pluginPeers = Invoke-JsonRpc -Url $GatewayUrl -Method "evm_getPublicBroadcastPluginPeers" -Params @{ chain_id = [UInt64]$ChainId }
    $nativeSync = Invoke-JsonRpc -Url $GatewayUrl -Method "evm_getRuntimeNativeSyncStatus" -Params @{ chain_id = [UInt64]$ChainId }

    $pluginReadyCount = [UInt64]0
    $pluginAuthSentCount = [UInt64]0
    $pluginDisconnectedCount = [UInt64]0
    $pluginLastError = ""
    $pluginPeerTop = @()
    $pluginWorkerReadyTotal = [UInt64]0
    $pluginWorkerNewHashesTotal = [UInt64]0
    $pluginWorkerUniqueNewHashesTotal = [UInt64]0
    $pluginWorkerDuplicateNewHashesTotal = [UInt64]0
    $pluginWorkerGetPooledTotal = [UInt64]0
    $pluginWorkerPooledTotal = [UInt64]0
    $pluginWorkerUniquePooledTotal = [UInt64]0
    $pluginWorkerDuplicatePooledTotal = [UInt64]0
    $pluginWorkerFirstSeenHashTotal = [UInt64]0
    $pluginWorkerFirstSeenTxTotal = [UInt64]0
    $pluginWorkerSwapTotal = [UInt64]0
    $pluginWorkerSwapV2Total = [UInt64]0
    $pluginWorkerSwapV3Total = [UInt64]0
    $pluginWorkerUniqueSwapTotal = [UInt64]0
    $pluginWorkerFirstPostReadyCode = $null
    [UInt64]$pluginTierCoreItems = 0
    [UInt64]$pluginTierActiveItems = 0
    [UInt64]$pluginTierCandidateItems = 0
    if ($null -ne $pluginPeers.items) {
        $scoredPeers = New-Object System.Collections.Generic.List[object]
        foreach ($item in @($pluginPeers.items)) {
            $stage = [string]$item.stage
            if ($stage -eq "ready") { $pluginReadyCount++ }
            if ($stage -eq "auth_sent") { $pluginAuthSentCount++ }
            if ($stage -eq "disconnected") { $pluginDisconnectedCount++ }
            if ([string]::IsNullOrWhiteSpace($pluginLastError)) {
                $err = [string]$item.last_error
                if (-not [string]::IsNullOrWhiteSpace($err)) {
                    $pluginLastError = $err
                }
            }
            [Int64]$scoreValue = 0
            $scoreRaw = ""
            if ($item.PSObject.Properties.Name -contains "score") {
                $scoreRaw = [string]$item.score
            }
            [void][Int64]::TryParse($scoreRaw, [ref]$scoreValue)
            $tierValue = "candidate"
            if ($item.PSObject.Properties.Name -contains "tier") {
                $tierValue = [string]$item.tier
            }
            switch ($tierValue.ToLowerInvariant()) {
                "core" { $pluginTierCoreItems++ }
                "active" { $pluginTierActiveItems++ }
                default { $pluginTierCandidateItems++ }
            }
            $readyCount = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "ready_count") {
                $readyCount = [UInt64](Convert-HexToUInt64 $item.ready_count)
            }
            $pluginWorkerReadyTotal = [UInt64]($pluginWorkerReadyTotal + $readyCount)
            $newHashesTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_new_pooled_hashes") {
                $newHashesTotal = [UInt64](Convert-HexToUInt64 $item.total_new_pooled_hashes)
            }
            $pluginWorkerNewHashesTotal = [UInt64]($pluginWorkerNewHashesTotal + $newHashesTotal)
            $uniqueNewHashesTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_unique_new_pooled_hashes") {
                $uniqueNewHashesTotal = [UInt64](Convert-HexToUInt64 $item.total_unique_new_pooled_hashes)
            }
            $pluginWorkerUniqueNewHashesTotal = [UInt64]($pluginWorkerUniqueNewHashesTotal + $uniqueNewHashesTotal)
            $duplicateNewHashesTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_duplicate_new_pooled_hashes") {
                $duplicateNewHashesTotal = [UInt64](Convert-HexToUInt64 $item.total_duplicate_new_pooled_hashes)
            }
            $pluginWorkerDuplicateNewHashesTotal = [UInt64]($pluginWorkerDuplicateNewHashesTotal + $duplicateNewHashesTotal)
            $getPooledTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_get_pooled_sent") {
                $getPooledTotal = [UInt64](Convert-HexToUInt64 $item.total_get_pooled_sent)
            }
            $pluginWorkerGetPooledTotal = [UInt64]($pluginWorkerGetPooledTotal + $getPooledTotal)
            $pooledTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "pooled_txs_total") {
                $pooledTotal = [UInt64](Convert-HexToUInt64 $item.pooled_txs_total)
            }
            $pluginWorkerPooledTotal = [UInt64]($pluginWorkerPooledTotal + $pooledTotal)
            $uniquePooledTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_unique_pooled_txs") {
                $uniquePooledTotal = [UInt64](Convert-HexToUInt64 $item.total_unique_pooled_txs)
            }
            $pluginWorkerUniquePooledTotal = [UInt64]($pluginWorkerUniquePooledTotal + $uniquePooledTotal)
            $duplicatePooledTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_duplicate_pooled_txs") {
                $duplicatePooledTotal = [UInt64](Convert-HexToUInt64 $item.total_duplicate_pooled_txs)
            }
            $pluginWorkerDuplicatePooledTotal = [UInt64]($pluginWorkerDuplicatePooledTotal + $duplicatePooledTotal)
            $firstSeenHashCount = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "first_seen_hash_count") {
                $firstSeenHashCount = [UInt64](Convert-HexToUInt64 $item.first_seen_hash_count)
            }
            $pluginWorkerFirstSeenHashTotal = [UInt64]($pluginWorkerFirstSeenHashTotal + $firstSeenHashCount)
            $firstSeenTxCount = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "first_seen_tx_count") {
                $firstSeenTxCount = [UInt64](Convert-HexToUInt64 $item.first_seen_tx_count)
            }
            $pluginWorkerFirstSeenTxTotal = [UInt64]($pluginWorkerFirstSeenTxTotal + $firstSeenTxCount)
            $swapTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_swap_hits") {
                $swapTotal = [UInt64](Convert-HexToUInt64 $item.total_swap_hits)
            }
            $pluginWorkerSwapTotal = [UInt64]($pluginWorkerSwapTotal + $swapTotal)
            $swapV2Total = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_swap_v2_hits") {
                $swapV2Total = [UInt64](Convert-HexToUInt64 $item.total_swap_v2_hits)
            }
            $pluginWorkerSwapV2Total = [UInt64]($pluginWorkerSwapV2Total + $swapV2Total)
            $swapV3Total = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_swap_v3_hits") {
                $swapV3Total = [UInt64](Convert-HexToUInt64 $item.total_swap_v3_hits)
            }
            $pluginWorkerSwapV3Total = [UInt64]($pluginWorkerSwapV3Total + $swapV3Total)
            $uniqueSwapTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "total_unique_swap_hits") {
                $uniqueSwapTotal = [UInt64](Convert-HexToUInt64 $item.total_unique_swap_hits)
            }
            $pluginWorkerUniqueSwapTotal = [UInt64]($pluginWorkerUniqueSwapTotal + $uniqueSwapTotal)
            $recentSwapTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "recent_swap_hits_total") {
                $recentSwapTotal = [UInt64](Convert-HexToUInt64 $item.recent_swap_hits_total)
            }
            $recentUniqueSwapTotal = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "recent_unique_swap_hits_total") {
                $recentUniqueSwapTotal = [UInt64](Convert-HexToUInt64 $item.recent_unique_swap_hits_total)
            }
            $avgFirstGossipLatencyMs = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "avg_first_gossip_latency_ms") {
                $avgFirstGossipLatencyMs = [UInt64](Convert-HexToUInt64 $item.avg_first_gossip_latency_ms)
            }
            $avgFirstSwapLatencyMs = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "avg_first_swap_latency_ms") {
                $avgFirstSwapLatencyMs = [UInt64](Convert-HexToUInt64 $item.avg_first_swap_latency_ms)
            }
            $disconnectCount = [UInt64]0
            if ($item.PSObject.Properties.Name -contains "disconnect_count") {
                $disconnectCount = [UInt64](Convert-HexToUInt64 $item.disconnect_count)
            }
            if ($null -eq $pluginWorkerFirstPostReadyCode -and $item.PSObject.Properties.Name -contains "last_first_post_ready_code") {
                $firstCodeRaw = [string]$item.last_first_post_ready_code
                if (-not [string]::IsNullOrWhiteSpace($firstCodeRaw)) {
                    $firstCodeValue = [UInt64](Convert-HexToUInt64 $firstCodeRaw)
                    if ($firstCodeValue -gt 0) {
                        $pluginWorkerFirstPostReadyCode = ("0x{0:x}" -f $firstCodeValue)
                    }
                }
            }
            $scoredPeers.Add([pscustomobject]@{
                endpoint = [string]$item.endpoint
                addr_hint = [string]$item.addr_hint
                tier = $tierValue
                score = [Int64]$scoreValue
                ready_count = $readyCount
                new_pooled_hashes_total = $newHashesTotal
                unique_new_pooled_hashes_total = $uniqueNewHashesTotal
                duplicate_new_pooled_hashes_total = $duplicateNewHashesTotal
                pooled_txs_total = $pooledTotal
                unique_pooled_txs_total = $uniquePooledTotal
                duplicate_pooled_txs_total = $duplicatePooledTotal
                first_seen_hash_count = $firstSeenHashCount
                first_seen_tx_count = $firstSeenTxCount
                swap_hits_total = $swapTotal
                swap_v2_hits_total = $swapV2Total
                swap_v3_hits_total = $swapV3Total
                unique_swap_hits_total = $uniqueSwapTotal
                recent_swap_hits_total = $recentSwapTotal
                recent_unique_swap_hits_total = $recentUniqueSwapTotal
                avg_first_gossip_latency_ms = $avgFirstGossipLatencyMs
                avg_first_swap_latency_ms = $avgFirstSwapLatencyMs
                disconnect_count = $disconnectCount
            })
        }
        if ($scoredPeers.Count -gt 0) {
            $sortProperties = @(
                @{ Expression = "score"; Descending = $true },
                @{ Expression = "unique_new_pooled_hashes_total"; Descending = $true },
                @{ Expression = "new_pooled_hashes_total"; Descending = $true },
                @{ Expression = "unique_pooled_txs_total"; Descending = $true },
                @{ Expression = "pooled_txs_total"; Descending = $true }
            )
            if ($EnableSwapPriority.IsPresent) {
                $sortProperties = @(
                    @{ Expression = "recent_unique_swap_hits_total"; Descending = $true },
                    @{ Expression = "recent_swap_hits_total"; Descending = $true },
                    @{ Expression = "unique_swap_hits_total"; Descending = $true },
                    @{ Expression = "swap_hits_total"; Descending = $true },
                    @{ Expression = "avg_first_swap_latency_ms"; Descending = $false },
                    @{ Expression = "score"; Descending = $true },
                    @{ Expression = "unique_new_pooled_hashes_total"; Descending = $true },
                    @{ Expression = "new_pooled_hashes_total"; Descending = $true }
                )
            }
            $pluginPeerTop = @(
                $scoredPeers |
                    Sort-Object -Property $sortProperties |
                    Select-Object -First 5
            )
        }
    }

    $v2Count = 0
    $v3Count = 0
    $sample = @()
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
                if ($sample.Count -lt [int]$SampleMax) {
                    $sample += [ordered]@{
                        from = $from
                        to = $to
                        nonce = [string]$nonceProp.Name
                        hash = $hash
                    }
                }
            }
        }
    }

    $pendingCount = [UInt64](Convert-HexToUInt64 $txpoolStatus.pending)
    $queuedCount = [UInt64](Convert-HexToUInt64 $txpoolStatus.queued)
    $ingressCount = [UInt64](Convert-HexToUInt64 $pendingIngress.count)
    $uniswapTotal = [UInt64]($v2Count + $v3Count)
    $otherPending = [UInt64]0
    if ($pendingCount -ge $uniswapTotal) {
        $otherPending = [UInt64]($pendingCount - $uniswapTotal)
    }

    return [ordered]@{
        observed_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
        peer_count = [UInt64](Convert-HexToUInt64 $peerCountHex)
        txpool_pending = $pendingCount
        txpool_queued = $queuedCount
        pending_ingress_count = $ingressCount
        plugin_total = [UInt64](Convert-HexToUInt64 $pluginPeers.total)
        plugin_reachable = [UInt64](Convert-HexToUInt64 $pluginPeers.reachable)
        plugin_peer_source = [string]$pluginPeers.peer_source
        plugin_session_ready = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_session_ready)
        plugin_session_ack_seen = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_session_ack_seen)
        plugin_session_auth_sent = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_session_auth_sent)
        plugin_session_tcp_connected = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_session_tcp_connected)
        plugin_ready_items = $pluginReadyCount
        plugin_auth_sent_items = $pluginAuthSentCount
        plugin_disconnected_items = $pluginDisconnectedCount
        plugin_last_error = $pluginLastError
        plugin_peer_top = $pluginPeerTop
        plugin_worker_ready_total = $pluginWorkerReadyTotal
        plugin_worker_new_hashes_total = $pluginWorkerNewHashesTotal
        plugin_worker_unique_new_hashes_total = $pluginWorkerUniqueNewHashesTotal
        plugin_worker_duplicate_new_hashes_total = $pluginWorkerDuplicateNewHashesTotal
        plugin_worker_get_pooled_total = $pluginWorkerGetPooledTotal
        plugin_worker_pooled_total = $pluginWorkerPooledTotal
        plugin_worker_unique_pooled_total = $pluginWorkerUniquePooledTotal
        plugin_worker_duplicate_pooled_total = $pluginWorkerDuplicatePooledTotal
        plugin_worker_first_seen_hash_total = $pluginWorkerFirstSeenHashTotal
        plugin_worker_first_seen_tx_total = $pluginWorkerFirstSeenTxTotal
        plugin_worker_swap_total = $pluginWorkerSwapTotal
        plugin_worker_swap_v2_total = $pluginWorkerSwapV2Total
        plugin_worker_swap_v3_total = $pluginWorkerSwapV3Total
        plugin_worker_unique_swap_total = $pluginWorkerUniqueSwapTotal
        plugin_worker_first_post_ready_code = $pluginWorkerFirstPostReadyCode
        plugin_tier_core_items = [UInt64]$pluginTierCoreItems
        plugin_tier_active_items = [UInt64]$pluginTierActiveItems
        plugin_tier_candidate_items = [UInt64]$pluginTierCandidateItems
        native_sync = $nativeSync
        uniswap_v2 = [UInt64]$v2Count
        uniswap_v3 = [UInt64]$v3Count
        uniswap_total = $uniswapTotal
        other_pending = $otherPending
        ingest_evicted_total = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_mempool_ingest_evicted_total)
        ingest_evicted_last_tick = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_mempool_ingest_evicted_last_tick)
        ingest_evicted_confirmed_last_tick = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_mempool_ingest_evicted_confirmed_last_tick)
        ingest_evicted_stale_last_tick = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_mempool_ingest_evicted_stale_last_tick)
        ingest_imported_total = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_mempool_ingest_imported_total)
        ingest_imported_last_tick = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_mempool_ingest_imported_last_tick)
        ingest_tick_count = [UInt64](Convert-HexToUInt64 $broadcastStatus.native_plugin_mempool_ingest_tick_count)
        ingest_last_error = [string]$broadcastStatus.native_plugin_mempool_ingest_last_error
        sample = $sample
    }
}

function Get-PeerTierRank {
    param([string]$Tier)
    $normalized = ([string]$Tier).Trim().ToLowerInvariant()
    if ($normalized -eq "core") { return 0 }
    if ($normalized -eq "active") { return 1 }
    return 2
}

function Get-ObjectFieldOrDefault {
    param(
        $Object,
        [string]$Name,
        $Default = $null
    )
    if ($null -eq $Object) {
        return $Default
    }
    if ($Object -is [System.Collections.IDictionary]) {
        if ($Object.Contains($Name)) {
            return $Object[$Name]
        }
        return $Default
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -ne $property) {
        return $property.Value
    }
    return $Default
}

function Build-PeerQualityTop {
    param(
        [object[]]$Observations,
        [int]$TopN = 10
    )
    $entries = @{}
    if ($null -eq $Observations) {
        return @()
    }
    $tick = 0
    foreach ($row in @($Observations)) {
        $tick++
        if ($null -eq $row) {
            continue
        }
        $peerListRaw = Get-ObjectFieldOrDefault -Object $row -Name "plugin_peer_top" -Default $null
        $peerList = @($peerListRaw)
        if ($peerList.Count -eq 0) {
            continue
        }
        foreach ($peer in $peerList) {
            if ($null -eq $peer) {
                continue
            }
            $endpoint = [string](Get-ObjectFieldOrDefault -Object $peer -Name "endpoint" -Default "")
            if ([string]::IsNullOrWhiteSpace($endpoint)) {
                continue
            }
            $addrHint = [string](Get-ObjectFieldOrDefault -Object $peer -Name "addr_hint" -Default "")
            $tier = [string](Get-ObjectFieldOrDefault -Object $peer -Name "tier" -Default "candidate")
            $tierRank = Get-PeerTierRank -Tier $tier
            [Int64]$score = 0
            [UInt64]$newHashes = 0
            [UInt64]$uniqueNewHashes = 0
            [UInt64]$duplicateNewHashes = 0
            [UInt64]$pooledTxs = 0
            [UInt64]$uniquePooledTxs = 0
            [UInt64]$duplicatePooledTxs = 0
            [UInt64]$firstSeenHashes = 0
            [UInt64]$firstSeenTxs = 0
            [UInt64]$swapHits = 0
            [UInt64]$swapV2Hits = 0
            [UInt64]$swapV3Hits = 0
            [UInt64]$uniqueSwapHits = 0
            [UInt64]$recentSwapHits = 0
            [UInt64]$recentUniqueSwapHits = 0
            [UInt64]$avgFirstGossipLatencyMs = 0
            [UInt64]$avgFirstSwapLatencyMs = 0
            [UInt64]$disconnectCount = 0
            [void][Int64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "score" -Default "0"), [ref]$score)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "new_pooled_hashes_total" -Default "0"), [ref]$newHashes)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "unique_new_pooled_hashes_total" -Default "0"), [ref]$uniqueNewHashes)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "duplicate_new_pooled_hashes_total" -Default "0"), [ref]$duplicateNewHashes)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "pooled_txs_total" -Default "0"), [ref]$pooledTxs)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "unique_pooled_txs_total" -Default "0"), [ref]$uniquePooledTxs)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "duplicate_pooled_txs_total" -Default "0"), [ref]$duplicatePooledTxs)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "first_seen_hash_count" -Default "0"), [ref]$firstSeenHashes)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "first_seen_tx_count" -Default "0"), [ref]$firstSeenTxs)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "swap_hits_total" -Default "0"), [ref]$swapHits)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "swap_v2_hits_total" -Default "0"), [ref]$swapV2Hits)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "swap_v3_hits_total" -Default "0"), [ref]$swapV3Hits)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "unique_swap_hits_total" -Default "0"), [ref]$uniqueSwapHits)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "recent_swap_hits_total" -Default "0"), [ref]$recentSwapHits)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "recent_unique_swap_hits_total" -Default "0"), [ref]$recentUniqueSwapHits)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "avg_first_gossip_latency_ms" -Default "0"), [ref]$avgFirstGossipLatencyMs)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "avg_first_swap_latency_ms" -Default "0"), [ref]$avgFirstSwapLatencyMs)
            [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $peer -Name "disconnect_count" -Default "0"), [ref]$disconnectCount)
            if (-not $entries.ContainsKey($endpoint)) {
                $entries[$endpoint] = [ordered]@{
                    endpoint = $endpoint
                    addr_hint = $addrHint
                    tier = $tier
                    tier_rank = [int]$tierRank
                    best_score = [Int64]$score
                    max_new_pooled_hashes_total = [UInt64]$newHashes
                    max_unique_new_pooled_hashes_total = [UInt64]$uniqueNewHashes
                    max_duplicate_new_pooled_hashes_total = [UInt64]$duplicateNewHashes
                    max_pooled_txs_total = [UInt64]$pooledTxs
                    max_unique_pooled_txs_total = [UInt64]$uniquePooledTxs
                    max_duplicate_pooled_txs_total = [UInt64]$duplicatePooledTxs
                    max_first_seen_hash_count = [UInt64]$firstSeenHashes
                    max_first_seen_tx_count = [UInt64]$firstSeenTxs
                    max_swap_hits_total = [UInt64]$swapHits
                    max_swap_v2_hits_total = [UInt64]$swapV2Hits
                    max_swap_v3_hits_total = [UInt64]$swapV3Hits
                    max_unique_swap_hits_total = [UInt64]$uniqueSwapHits
                    max_recent_swap_hits_total = [UInt64]$recentSwapHits
                    max_recent_unique_swap_hits_total = [UInt64]$recentUniqueSwapHits
                    min_avg_first_gossip_latency_ms = [UInt64]$avgFirstGossipLatencyMs
                    min_avg_first_swap_latency_ms = [UInt64]$avgFirstSwapLatencyMs
                    max_disconnect_count = [UInt64]$disconnectCount
                    top_hits = [UInt64]1
                    first_seen_tick = [UInt64]$tick
                    last_seen_tick = [UInt64]$tick
                }
                continue
            }
            $entry = $entries[$endpoint]
            $entry.top_hits = [UInt64]$entry.top_hits + 1
            $entry.last_seen_tick = [UInt64]$tick
            if ([string]::IsNullOrWhiteSpace([string]$entry.addr_hint) -and -not [string]::IsNullOrWhiteSpace($addrHint)) {
                $entry.addr_hint = $addrHint
            }
            if ([int]$tierRank -lt [int]$entry.tier_rank) {
                $entry.tier_rank = [int]$tierRank
                $entry.tier = $tier
            }
            if ([Int64]$score -gt [Int64]$entry.best_score) {
                $entry.best_score = [Int64]$score
            }
            if ([UInt64]$newHashes -gt [UInt64]$entry.max_new_pooled_hashes_total) {
                $entry.max_new_pooled_hashes_total = [UInt64]$newHashes
            }
            if ([UInt64]$uniqueNewHashes -gt [UInt64]$entry.max_unique_new_pooled_hashes_total) {
                $entry.max_unique_new_pooled_hashes_total = [UInt64]$uniqueNewHashes
            }
            if ([UInt64]$duplicateNewHashes -gt [UInt64]$entry.max_duplicate_new_pooled_hashes_total) {
                $entry.max_duplicate_new_pooled_hashes_total = [UInt64]$duplicateNewHashes
            }
            if ([UInt64]$pooledTxs -gt [UInt64]$entry.max_pooled_txs_total) {
                $entry.max_pooled_txs_total = [UInt64]$pooledTxs
            }
            if ([UInt64]$uniquePooledTxs -gt [UInt64]$entry.max_unique_pooled_txs_total) {
                $entry.max_unique_pooled_txs_total = [UInt64]$uniquePooledTxs
            }
            if ([UInt64]$duplicatePooledTxs -gt [UInt64]$entry.max_duplicate_pooled_txs_total) {
                $entry.max_duplicate_pooled_txs_total = [UInt64]$duplicatePooledTxs
            }
            if ([UInt64]$firstSeenHashes -gt [UInt64]$entry.max_first_seen_hash_count) {
                $entry.max_first_seen_hash_count = [UInt64]$firstSeenHashes
            }
            if ([UInt64]$firstSeenTxs -gt [UInt64]$entry.max_first_seen_tx_count) {
                $entry.max_first_seen_tx_count = [UInt64]$firstSeenTxs
            }
            if ([UInt64]$swapHits -gt [UInt64]$entry.max_swap_hits_total) {
                $entry.max_swap_hits_total = [UInt64]$swapHits
            }
            if ([UInt64]$swapV2Hits -gt [UInt64]$entry.max_swap_v2_hits_total) {
                $entry.max_swap_v2_hits_total = [UInt64]$swapV2Hits
            }
            if ([UInt64]$swapV3Hits -gt [UInt64]$entry.max_swap_v3_hits_total) {
                $entry.max_swap_v3_hits_total = [UInt64]$swapV3Hits
            }
            if ([UInt64]$uniqueSwapHits -gt [UInt64]$entry.max_unique_swap_hits_total) {
                $entry.max_unique_swap_hits_total = [UInt64]$uniqueSwapHits
            }
            if ([UInt64]$recentSwapHits -gt [UInt64]$entry.max_recent_swap_hits_total) {
                $entry.max_recent_swap_hits_total = [UInt64]$recentSwapHits
            }
            if ([UInt64]$recentUniqueSwapHits -gt [UInt64]$entry.max_recent_unique_swap_hits_total) {
                $entry.max_recent_unique_swap_hits_total = [UInt64]$recentUniqueSwapHits
            }
            if (([UInt64]$entry.min_avg_first_gossip_latency_ms -eq 0 -or [UInt64]$avgFirstGossipLatencyMs -lt [UInt64]$entry.min_avg_first_gossip_latency_ms) -and [UInt64]$avgFirstGossipLatencyMs -gt 0) {
                $entry.min_avg_first_gossip_latency_ms = [UInt64]$avgFirstGossipLatencyMs
            }
            if (([UInt64]$entry.min_avg_first_swap_latency_ms -eq 0 -or [UInt64]$avgFirstSwapLatencyMs -lt [UInt64]$entry.min_avg_first_swap_latency_ms) -and [UInt64]$avgFirstSwapLatencyMs -gt 0) {
                $entry.min_avg_first_swap_latency_ms = [UInt64]$avgFirstSwapLatencyMs
            }
            if ([UInt64]$disconnectCount -gt [UInt64]$entry.max_disconnect_count) {
                $entry.max_disconnect_count = [UInt64]$disconnectCount
            }
        }
    }
    if ($entries.Count -eq 0) {
        return @()
    }
    $rows = @($entries.Values | ForEach-Object { [pscustomobject]$_ })
    return @(
        $rows |
            Sort-Object -Property `
                @{ Expression = "tier_rank"; Descending = $false }, `
                @{ Expression = "max_recent_unique_swap_hits_total"; Descending = $true }, `
                @{ Expression = "max_recent_swap_hits_total"; Descending = $true }, `
                @{ Expression = "max_unique_swap_hits_total"; Descending = $true }, `
                @{ Expression = "best_score"; Descending = $true }, `
                @{ Expression = "max_unique_new_pooled_hashes_total"; Descending = $true }, `
                @{ Expression = "max_new_pooled_hashes_total"; Descending = $true }, `
                @{ Expression = "max_unique_pooled_txs_total"; Descending = $true }, `
                @{ Expression = "max_pooled_txs_total"; Descending = $true }, `
                @{ Expression = "top_hits"; Descending = $true }, `
                @{ Expression = "endpoint"; Descending = $false } |
            Select-Object -First ([int][Math]::Max(1, $TopN))
    )
}

function Build-PeerTopStabilityStats {
    param([object[]]$Observations)
    $rows = @($Observations | Where-Object { $null -ne $_ })
    $ticks = 0
    [UInt64]$top1CoreHits = 0
    [UInt64]$top1ActiveHits = 0
    [UInt64]$top1CandidateHits = 0
    [UInt64]$top1OtherHits = 0
    [UInt64]$top1Switches = 0
    [UInt64]$coreStreak = 0
    [UInt64]$maxCoreStreak = 0
    $prevTopEndpoint = ""
    foreach ($row in $rows) {
        $peerList = @(Get-ObjectFieldOrDefault -Object $row -Name "plugin_peer_top" -Default @())
        if ($peerList.Count -eq 0) {
            continue
        }
        $ticks++
        $top = $peerList[0]
        $endpoint = [string](Get-ObjectFieldOrDefault -Object $top -Name "endpoint" -Default "")
        if (-not [string]::IsNullOrWhiteSpace($prevTopEndpoint) -and -not [string]::IsNullOrWhiteSpace($endpoint) -and $endpoint -ne $prevTopEndpoint) {
            $top1Switches++
        }
        if (-not [string]::IsNullOrWhiteSpace($endpoint)) {
            $prevTopEndpoint = $endpoint
        }
        $tier = ([string](Get-ObjectFieldOrDefault -Object $top -Name "tier" -Default "candidate")).ToLowerInvariant()
        switch ($tier) {
            "core" {
                $top1CoreHits++
                $coreStreak++
                if ($coreStreak -gt $maxCoreStreak) {
                    $maxCoreStreak = $coreStreak
                }
            }
            "active" {
                $top1ActiveHits++
                $coreStreak = 0
            }
            "candidate" {
                $top1CandidateHits++
                $coreStreak = 0
            }
            default {
                $top1OtherHits++
                $coreStreak = 0
            }
        }
    }
    $tickFloat = if ($ticks -gt 0) { [double]$ticks } else { [double]1 }
    return [ordered]@{
        ticks = [UInt64]$ticks
        top1_core_hits = [UInt64]$top1CoreHits
        top1_active_hits = [UInt64]$top1ActiveHits
        top1_candidate_hits = [UInt64]$top1CandidateHits
        top1_other_hits = [UInt64]$top1OtherHits
        top1_core_hit_rate_pct = if ($ticks -gt 0) { [Math]::Round((100.0 * [double]$top1CoreHits) / $tickFloat, 2) } else { [double]0 }
        top1_active_hit_rate_pct = if ($ticks -gt 0) { [Math]::Round((100.0 * [double]$top1ActiveHits) / $tickFloat, 2) } else { [double]0 }
        top1_candidate_hit_rate_pct = if ($ticks -gt 0) { [Math]::Round((100.0 * [double]$top1CandidateHits) / $tickFloat, 2) } else { [double]0 }
        top1_peer_switches = [UInt64]$top1Switches
        top1_core_max_streak = [UInt64]$maxCoreStreak
    }
}

function Build-PeerContributionStats {
    param([object[]]$PeerQualityTop)
    $rows = @($PeerQualityTop | Where-Object { $null -ne $_ })
    [UInt64]$totalHashes = 0
    [UInt64]$totalUniqueHashes = 0
    [UInt64]$totalDuplicateHashes = 0
    [UInt64]$totalFirstSeenHashes = 0
    foreach ($row in $rows) {
        [UInt64]$value = 0
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "max_new_pooled_hashes_total" -Default "0"), [ref]$value)
        $totalHashes = [UInt64]($totalHashes + $value)
        [UInt64]$uniqueValue = 0
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "max_unique_new_pooled_hashes_total" -Default "0"), [ref]$uniqueValue)
        $totalUniqueHashes = [UInt64]($totalUniqueHashes + $uniqueValue)
        [UInt64]$duplicateValue = 0
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "max_duplicate_new_pooled_hashes_total" -Default "0"), [ref]$duplicateValue)
        $totalDuplicateHashes = [UInt64]($totalDuplicateHashes + $duplicateValue)
        [UInt64]$firstSeenValue = 0
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "max_first_seen_hash_count" -Default "0"), [ref]$firstSeenValue)
        $totalFirstSeenHashes = [UInt64]($totalFirstSeenHashes + $firstSeenValue)
    }
    if ($totalHashes -eq 0) {
        return [ordered]@{
            total_hashes = [UInt64]0
            total_unique_hashes = [UInt64]0
            total_duplicate_hashes = [UInt64]0
            total_first_seen_hashes = [UInt64]0
            top1_hash_share_pct = [double]0
            top3_hash_share_pct = [double]0
            top5_hash_share_pct = [double]0
            top1_unique_hash_share_pct = [double]0
            top3_unique_hash_share_pct = [double]0
            top5_unique_hash_share_pct = [double]0
        }
    }
    $sorted = @(
        $rows |
            Sort-Object -Property @{ Expression = "max_new_pooled_hashes_total"; Descending = $true }, @{ Expression = "max_unique_new_pooled_hashes_total"; Descending = $true }, @{ Expression = "max_pooled_txs_total"; Descending = $true }
    )
    $sortedUnique = @(
        $rows |
            Sort-Object -Property @{ Expression = "max_unique_new_pooled_hashes_total"; Descending = $true }, @{ Expression = "max_new_pooled_hashes_total"; Descending = $true }, @{ Expression = "max_first_seen_hash_count"; Descending = $true }
    )
    [UInt64]$top1 = 0
    [UInt64]$top3 = 0
    [UInt64]$top5 = 0
    for ($i = 0; $i -lt $sorted.Count; $i++) {
        [UInt64]$value = 0
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $sorted[$i] -Name "max_new_pooled_hashes_total" -Default "0"), [ref]$value)
        if ($i -lt 1) { $top1 = [UInt64]($top1 + $value) }
        if ($i -lt 3) { $top3 = [UInt64]($top3 + $value) }
        if ($i -lt 5) { $top5 = [UInt64]($top5 + $value) }
    }
    [UInt64]$top1Unique = 0
    [UInt64]$top3Unique = 0
    [UInt64]$top5Unique = 0
    for ($i = 0; $i -lt $sortedUnique.Count; $i++) {
        [UInt64]$value = 0
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $sortedUnique[$i] -Name "max_unique_new_pooled_hashes_total" -Default "0"), [ref]$value)
        if ($i -lt 1) { $top1Unique = [UInt64]($top1Unique + $value) }
        if ($i -lt 3) { $top3Unique = [UInt64]($top3Unique + $value) }
        if ($i -lt 5) { $top5Unique = [UInt64]($top5Unique + $value) }
    }
    $totalFloat = [double]$totalHashes
    $totalUniqueFloat = if ($totalUniqueHashes -gt 0) { [double]$totalUniqueHashes } else { [double]1 }
    return [ordered]@{
        total_hashes = [UInt64]$totalHashes
        total_unique_hashes = [UInt64]$totalUniqueHashes
        total_duplicate_hashes = [UInt64]$totalDuplicateHashes
        total_first_seen_hashes = [UInt64]$totalFirstSeenHashes
        top1_hash_share_pct = [Math]::Round((100.0 * [double]$top1) / $totalFloat, 2)
        top3_hash_share_pct = [Math]::Round((100.0 * [double]$top3) / $totalFloat, 2)
        top5_hash_share_pct = [Math]::Round((100.0 * [double]$top5) / $totalFloat, 2)
        top1_unique_hash_share_pct = if ($totalUniqueHashes -gt 0) { [Math]::Round((100.0 * [double]$top1Unique) / $totalUniqueFloat, 2) } else { [double]0 }
        top3_unique_hash_share_pct = if ($totalUniqueHashes -gt 0) { [Math]::Round((100.0 * [double]$top3Unique) / $totalUniqueFloat, 2) } else { [double]0 }
        top5_unique_hash_share_pct = if ($totalUniqueHashes -gt 0) { [Math]::Round((100.0 * [double]$top5Unique) / $totalUniqueFloat, 2) } else { [double]0 }
    }
}

function Build-CoreUniqueDistribution {
    param(
        [object[]]$PeerQualityTop,
        [int]$TopN = 8
    )
    $rows = @($PeerQualityTop | Where-Object { $null -ne $_ -and ([string]$_.tier).ToLowerInvariant() -eq "core" })
    if ($rows.Count -eq 0) {
        return @()
    }
    $sorted = @(
        $rows |
            Sort-Object -Property `
                @{ Expression = "max_unique_new_pooled_hashes_total"; Descending = $true }, `
                @{ Expression = "max_first_seen_hash_count"; Descending = $true }, `
                @{ Expression = "best_score"; Descending = $true }, `
                @{ Expression = "endpoint"; Descending = $false }
    )
    $top = @($sorted | Select-Object -First ([int][Math]::Max(1, $TopN)))
    $out = @()
    foreach ($item in $top) {
        [UInt64]$uniqueHashes = 0
        [UInt64]$duplicateHashes = 0
        [UInt64]$firstSeenHashes = 0
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $item -Name "max_unique_new_pooled_hashes_total" -Default "0"), [ref]$uniqueHashes)
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $item -Name "max_duplicate_new_pooled_hashes_total" -Default "0"), [ref]$duplicateHashes)
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $item -Name "max_first_seen_hash_count" -Default "0"), [ref]$firstSeenHashes)
        $dupRatio = if (($uniqueHashes + $duplicateHashes) -gt 0) {
            [Math]::Round((100.0 * [double]$duplicateHashes) / [double]($uniqueHashes + $duplicateHashes), 2)
        } else {
            [double]0
        }
        $out += [pscustomobject]@{
            endpoint = [string]$item.endpoint
            addr_hint = [string]$item.addr_hint
            score = [Int64]$item.best_score
            unique_hashes = [UInt64]$uniqueHashes
            duplicate_hashes = [UInt64]$duplicateHashes
            first_seen_hashes = [UInt64]$firstSeenHashes
            duplicate_ratio_pct = $dupRatio
        }
    }
    return @($out)
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$SummaryDir = Split-Path -Parent $SummaryOut
if ($SummaryDir) {
    New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
}
$SeedSummaryPath = Resolve-FullPath -Root $RepoRoot -Value $SeedSummaryPath

$GatewayExe = $null
if (-not $AttachExistingGateway.IsPresent) {
    $TargetRoot = if ($env:CARGO_TARGET_DIR) {
        [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
    } else {
        Join-Path $RepoRoot "target"
    }
    if (-not $SkipBuild) {
        Push-Location $RepoRoot
        try {
            & cargo build -p novovm-evm-gateway
            if ($LASTEXITCODE -ne 0) {
                throw "build failed: novovm-evm-gateway"
            }
        } finally {
            Pop-Location
        }
    }
    $GatewayExe = Resolve-BinaryPath -TargetRoot $TargetRoot -BinaryBaseName "novovm-evm-gateway"
    if (-not (Test-Path $GatewayExe)) {
        throw "gateway binary not found: $GatewayExe"
    }
    Write-Host ("gateway exe: {0}" -f $GatewayExe)
}

$logDir = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$runTag = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$gwOut = Join-Path $logDir "evm-uniswap-observation-window-gateway.stdout.log"
$gwErr = Join-Path $logDir "evm-uniswap-observation-window-gateway.stderr.log"
if (Test-Path $gwOut) {
    try {
        Remove-Item -Force -ErrorAction Stop $gwOut
    } catch {
        $gwOut = Join-Path $logDir ("evm-uniswap-observation-window-gateway.stdout.{0}.log" -f $runTag)
        Write-Warning ("stdout log is busy; using timestamped log path: {0}" -f $gwOut)
    }
}
if (Test-Path $gwErr) {
    try {
        Remove-Item -Force -ErrorAction Stop $gwErr
    } catch {
        $gwErr = Join-Path $logDir ("evm-uniswap-observation-window-gateway.stderr.{0}.log" -f $runTag)
        Write-Warning ("stderr log is busy; using timestamped log path: {0}" -f $gwErr)
    }
}

$runTmpDir = Resolve-FullPath -Root $RepoRoot -Value ("artifacts/migration/tmp/evm-uniswap-observe-{0}" -f $runTag)
if (Test-Path $runTmpDir) {
    Remove-Item -Recurse -Force $runTmpDir
}
New-Item -ItemType Directory -Force -Path $runTmpDir | Out-Null
$gatewayUaStorePath = Join-Path $runTmpDir "gateway-ua-store.bin"
$gatewaySpoolDir = Join-Path $runTmpDir "gateway-spool"
New-Item -ItemType Directory -Force -Path $gatewaySpoolDir | Out-Null
$gatewayRlpxProfilePath = $null
if ([string]::IsNullOrWhiteSpace([string]$RlpxProfilePath)) {
    $gatewayRlpxProfilePath = Resolve-FullPath -Root $RepoRoot -Value ("artifacts/migration/state/gateway-eth-rlpx-peer-profile-chain-{0}.json" -f $ChainId)
} else {
    $gatewayRlpxProfilePath = Resolve-FullPath -Root $RepoRoot -Value ([string]$RlpxProfilePath)
}
$gatewayRlpxProfileDir = Split-Path -Parent $gatewayRlpxProfilePath
if (-not [string]::IsNullOrWhiteSpace([string]$gatewayRlpxProfileDir)) {
    New-Item -ItemType Directory -Force -Path $gatewayRlpxProfileDir | Out-Null
}
if ($FreshRlpxProfile.IsPresent -and (Test-Path $gatewayRlpxProfilePath)) {
    Remove-Item -Force -Path $gatewayRlpxProfilePath
}

$chainHex = ("0x{0:x}" -f $ChainId)
$hasFixedPluginEnode = -not [string]::IsNullOrWhiteSpace([string]$FixedPluginEnode)
$dnsDiscoveryEnabled = [bool]$EnableDnsDiscoverySeed -and ([UInt64]$ChainId -eq [UInt64]1) -and (-not $hasFixedPluginEnode)
$dnsSeedEnodes = @()
$dnsSeedError = $null
$bootnodesIncludedCount = 0
$bootnodesSkippedForEnvLimit = 0
$bootnodesEnvValueLength = 0
if ($dnsDiscoveryEnabled) {
    $dnsResult = Resolve-DnsDiscoveryEnodes -Root $RepoRoot -MaxEnodes $DnsDiscoveryMaxEnodes -DnsRoot $DnsDiscoveryRoot
    if ($null -ne $dnsResult.error) {
        $dnsSeedError = [string]$dnsResult.error
    }
    if ($null -ne $dnsResult.enodes) {
        $dnsSeedEnodes = @($dnsResult.enodes)
    }
    Write-Host ("dns seed: enabled=true root={0} count={1} error={2}" -f $DnsDiscoveryRoot, $dnsSeedEnodes.Count, $dnsSeedError)
}
if ($hasFixedPluginEnode) {
    Write-Host ("fixed enode mode: endpoint={0}" -f $FixedPluginEnode)
}

$envMap = @{
    "NOVOVM_GATEWAY_BIND" = $GatewayBind
    "NOVOVM_GATEWAY_WARN_LOG" = "1"
    "NOVOVM_GATEWAY_UA_STORE_BACKEND" = "bincode_file"
    "NOVOVM_GATEWAY_UA_STORE_PATH" = $gatewayUaStorePath
    "NOVOVM_GATEWAY_SPOOL_DIR" = $gatewaySpoolDir
    "NOVOVM_GATEWAY_ETH_DEFAULT_CHAIN_ID" = ([string]$ChainId)
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ROUTE_POLICY" = "auto"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ROUTE_POLICY_CHAIN_$ChainId" = "auto"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ROUTE_POLICY_CHAIN_$chainHex" = "auto"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS" = $PluginPorts
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS_CHAIN_$ChainId" = $PluginPorts
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS_CHAIN_$chainHex" = $PluginPorts
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_SESSION_PROBE_MODE" = "off"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_SESSION_PROBE_MODE_CHAIN_$ChainId" = "off"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_SESSION_PROBE_MODE_CHAIN_$chainHex" = "off"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_MIN_CANDIDATES" = ([string]([UInt64][Math]::Max(1, $PluginMinCandidates)))
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_MIN_CANDIDATES_CHAIN_$ChainId" = ([string]([UInt64][Math]::Max(1, $PluginMinCandidates)))
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_MIN_CANDIDATES_CHAIN_$chainHex" = ([string]([UInt64][Math]::Max(1, $PluginMinCandidates)))
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES" = "1"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES_CHAIN_$ChainId" = "1"
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES_CHAIN_$chainHex" = "1"
    "NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND" = "memory"
}
Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_PROFILE_PATH" -ChainId $ChainId -ChainHex $chainHex -Value $gatewayRlpxProfilePath
$existingBootnodes = [string]$env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_BOOTNODES
$mergedBootnodes = @()
if ($hasFixedPluginEnode) {
    foreach ($token in ([string]$FixedPluginEnode).Split([char[]]",;`n`r`t ", [System.StringSplitOptions]::RemoveEmptyEntries)) {
        $item = [string]$token
        if ([string]::IsNullOrWhiteSpace($item)) {
            continue
        }
        $mergedBootnodes += $item.Trim()
    }
}
if ($dnsSeedEnodes.Count -gt 0) {
    $mergedBootnodes += $dnsSeedEnodes
}
if ((-not $hasFixedPluginEnode) -and (-not [string]::IsNullOrWhiteSpace($existingBootnodes))) {
    $mergedBootnodes += $existingBootnodes.Split([char[]]",;`n`r`t ", [System.StringSplitOptions]::RemoveEmptyEntries)
}
if ($hasFixedPluginEnode) {
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES"] = "0"
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES_CHAIN_$ChainId"] = "0"
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES_CHAIN_$chainHex"] = "0"
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_MIN_CANDIDATES"] = "1"
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_MIN_CANDIDATES_CHAIN_$ChainId"] = "1"
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_MIN_CANDIDATES_CHAIN_$chainHex"] = "1"
    $portMatch = [regex]::Match([string]$FixedPluginEnode, "@[^:@\s]+:(\d+)")
    if ($portMatch.Success) {
        $fixedPort = [string]$portMatch.Groups[1].Value
        $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS"] = $fixedPort
        $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS_CHAIN_$ChainId"] = $fixedPort
        $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_PLUGIN_PORTS_CHAIN_$chainHex"] = $fixedPort
    }
}
if ($mergedBootnodes.Count -gt 0) {
    $bootnodeDedup = New-Object System.Collections.Generic.HashSet[string]
    $bootnodeFinal = New-Object System.Collections.Generic.List[string]
    foreach ($entry in $mergedBootnodes) {
        $trimmed = [string]$entry
        if ([string]::IsNullOrWhiteSpace($trimmed)) {
            continue
        }
        $item = $trimmed.Trim()
        if ($bootnodeDedup.Add($item.ToLowerInvariant())) {
            $bootnodeFinal.Add($item)
        }
    }
    if ($bootnodeFinal.Count -gt 0) {
        $bootnodeCsv = Join-BoundedCsv -Items $bootnodeFinal -MaxLength 28000
        $bootnodesIncludedCount = [int]$bootnodeCsv.included
        $bootnodesSkippedForEnvLimit = [int]$bootnodeCsv.skipped
        $bootnodesEnvValueLength = [int]$bootnodeCsv.length
        if ($bootnodeCsv.included -gt 0) {
            $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_BOOTNODES"] = [string]$bootnodeCsv.csv
        }
        if ($bootnodeCsv.skipped -gt 0) {
            Write-Warning ("bootnodes env truncated by length cap: included={0} skipped={1} len={2}" -f $bootnodeCsv.included, $bootnodeCsv.skipped, $bootnodeCsv.length)
        }
    }
}

$enableIngest = $EnablePluginMempoolIngest.IsPresent
if ($enableIngest) {
    Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_ENABLE" -ChainId $ChainId -ChainHex $chainHex -Value "1"
    Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_POLL_MS" -ChainId $ChainId -ChainHex $chainHex -Value ([string]$PluginMempoolPollMs)
    $maxPeersPerTick = ([string]([UInt64][Math]::Max(1, $RlpxMaxPeersPerTick)))
    Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_MAX_PEERS_PER_TICK" -ChainId $ChainId -ChainHex $chainHex -Value $maxPeersPerTick
    if ($null -ne $RlpxCoreTarget) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_TARGET" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxCoreTarget)
    }
    if ($null -ne $RlpxActiveTarget) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_ACTIVE_TARGET" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxActiveTarget)
    }
    if ($null -ne $RlpxCoreRecentGossipWindowMs) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_RECENT_GOSSIP_WINDOW_MS" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxCoreRecentGossipWindowMs)
    }
    if ($null -ne $RlpxActiveRecentReadyWindowMs) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_ACTIVE_RECENT_READY_WINDOW_MS" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxActiveRecentReadyWindowMs)
    }
    if ($null -ne $RlpxCoreLockMs) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_CORE_LOCK_MS" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxCoreLockMs)
    }
    if ($null -ne $RlpxRecentNewHashWindowMs) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_RECENT_NEW_HASH_WINDOW_MS" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxRecentNewHashWindowMs)
    }
    if ($null -ne $RlpxRecentNewHashMin) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_RECENT_NEW_HASH_MIN" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxRecentNewHashMin)
    }
    if ($null -ne $RlpxPriorityBudget) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_PRIORITY_BUDGET" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxPriorityBudget)
    }
    if ($null -ne $RlpxPriorityAutoPoolSize) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_PRIORITY_AUTO_POOL_SIZE" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxPriorityAutoPoolSize)
    }
    if ($RlpxSingleSession.IsPresent) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SINGLE_SESSION" -ChainId $ChainId -ChainHex $chainHex -Value "1"
    }
    if ($EnableSwapPriority.IsPresent) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SWAP_PRIORITY_ENABLE" -ChainId $ChainId -ChainHex $chainHex -Value "1"
    }
    if ($null -ne $RlpxSwapPriorityLatencyTargetMs) {
        Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_SWAP_PRIORITY_LATENCY_TARGET_MS" -ChainId $ChainId -ChainHex $chainHex -Value ([string][UInt64]$RlpxSwapPriorityLatencyTargetMs)
    }
}
if (-not [string]::IsNullOrWhiteSpace([string]$RlpxHelloProfile)) {
    $helloProfile = ([string]$RlpxHelloProfile).Trim()
    Set-ChainScopedEnvValue -Environment $envMap -BaseKey "NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_HELLO_PROFILE" -ChainId $ChainId -ChainHex $chainHex -Value $helloProfile
}

$summary = [ordered]@{
    started_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    attach_existing_gateway = [bool]$AttachExistingGateway
    gateway_bind = $GatewayBind
    chain_id = [UInt64]$ChainId
    duration_minutes = [UInt64]$DurationMinutes
    interval_seconds = [UInt64]$IntervalSeconds
    warmup_seconds = [UInt64]$WarmupSeconds
    plugin_ports = $PluginPorts
    seed_from_canary_summary = [bool]$SeedFromCanarySummary
    seed_summary_path = $SeedSummaryPath
    seed_max_import = [UInt64]$SeedMaxImport
    dns_discovery_seed_enabled = [bool]$dnsDiscoveryEnabled
    dns_discovery_seed_root = $DnsDiscoveryRoot
    dns_discovery_seed_requested = [UInt64]$DnsDiscoveryMaxEnodes
    dns_discovery_seed_count = [UInt64]$dnsSeedEnodes.Count
    dns_discovery_seed_error = $dnsSeedError
    bootnodes_env_included_count = [UInt64][Math]::Max(0, $bootnodesIncludedCount)
    bootnodes_env_skipped_for_length = [UInt64][Math]::Max(0, $bootnodesSkippedForEnvLimit)
    bootnodes_env_value_length = [UInt64][Math]::Max(0, $bootnodesEnvValueLength)
    fixed_plugin_enode = if ($hasFixedPluginEnode) { [string]$FixedPluginEnode } else { $null }
    plugin_mempool_ingest_enabled = [bool]$EnablePluginMempoolIngest
    plugin_mempool_poll_ms = [UInt64]$PluginMempoolPollMs
    plugin_min_candidates = [UInt64][Math]::Max(1, $PluginMinCandidates)
    rlpx_max_peers_per_tick = [UInt64][Math]::Max(1, $RlpxMaxPeersPerTick)
    rlpx_hello_profile = [string]$RlpxHelloProfile
    rlpx_profile_path = [string]$gatewayRlpxProfilePath
    rlpx_profile_fresh_start = [bool]$FreshRlpxProfile
    rlpx_single_session = [bool]$RlpxSingleSession
    rlpx_core_target = if ($null -eq $RlpxCoreTarget) { $null } else { [UInt64]$RlpxCoreTarget }
    rlpx_active_target = if ($null -eq $RlpxActiveTarget) { $null } else { [UInt64]$RlpxActiveTarget }
    rlpx_core_recent_gossip_window_ms = if ($null -eq $RlpxCoreRecentGossipWindowMs) { $null } else { [UInt64]$RlpxCoreRecentGossipWindowMs }
    rlpx_active_recent_ready_window_ms = if ($null -eq $RlpxActiveRecentReadyWindowMs) { $null } else { [UInt64]$RlpxActiveRecentReadyWindowMs }
    rlpx_core_lock_ms = if ($null -eq $RlpxCoreLockMs) { $null } else { [UInt64]$RlpxCoreLockMs }
    rlpx_recent_new_hash_window_ms = if ($null -eq $RlpxRecentNewHashWindowMs) { $null } else { [UInt64]$RlpxRecentNewHashWindowMs }
    rlpx_recent_new_hash_min = if ($null -eq $RlpxRecentNewHashMin) { $null } else { [UInt64]$RlpxRecentNewHashMin }
    rlpx_priority_budget = if ($null -eq $RlpxPriorityBudget) { $null } else { [UInt64]$RlpxPriorityBudget }
    rlpx_priority_auto_pool_size = if ($null -eq $RlpxPriorityAutoPoolSize) { $null } else { [UInt64]$RlpxPriorityAutoPoolSize }
    rlpx_swap_priority_enabled = [bool]$EnableSwapPriority
    rlpx_swap_priority_latency_target_ms = if ($null -eq $RlpxSwapPriorityLatencyTargetMs) { $null } else { [UInt64]$RlpxSwapPriorityLatencyTargetMs }
    smoke_assert_enabled = [bool]$SmokeAssert
    smoke_min_ready = [UInt64][Math]::Max(0, $SmokeMinReady)
    smoke_min_new_pooled = [UInt64][Math]::Max(0, $SmokeMinNewPooled)
    smoke_min_pooled = [UInt64][Math]::Max(0, $SmokeMinPooled)
    smoke_require_first_frame_new_pooled = [bool]$SmokeRequireFirstFrameNewPooled
    strict_public_connectivity = $true
    connectivity_guard_error = $null
    seed_result = $null
    observations = @()
    aggregate = [ordered]@{
        max_peer_count = 0
        max_txpool_pending = 0
        max_pending_ingress = 0
        max_uniswap_v2 = 0
        max_uniswap_v3 = 0
        max_uniswap_total = 0
        max_other_pending = 0
        max_pending_delta = 0
        min_pending_delta = 0
        max_ingest_evicted_total = 0
        max_ingest_evicted_last_tick = 0
        max_plugin_reachable = 0
        max_plugin_tier_core_items = 0
        max_plugin_tier_active_items = 0
        max_plugin_tier_candidate_items = 0
        error_ticks = 0
    }
    gateway_stdout = $gwOut
    gateway_stderr = $gwErr
    gateway_exe = if ($null -ne $GatewayExe) { [string]$GatewayExe } else { $null }
    run_tmp_dir = $runTmpDir
    gateway_ua_store_path = $gatewayUaStorePath
    gateway_spool_dir = $gatewaySpoolDir
    gateway_rlpx_profile_path = $gatewayRlpxProfilePath
}
$summary.env_overrides = [ordered]@{}
foreach ($key in @($envMap.Keys | Sort-Object)) {
    if (
        $key.StartsWith("NOVOVM_GATEWAY_ETH_PLUGIN_MEMPOOL_INGEST_RLPX_", [System.StringComparison]::Ordinal) -or
        $key.StartsWith("NOVOVM_GATEWAY_ETH_PLUGIN_RLPX_HELLO_PROFILE", [System.StringComparison]::Ordinal)
    ) {
        $summary.env_overrides[$key] = [string]$envMap[$key]
    }
}

$gatewayProc = $null
try {
    if (-not $AttachExistingGateway.IsPresent) {
        $envState = Push-ProcessEnv -Environment $envMap
        try {
            $gatewayProc = Start-Process `
                -FilePath $GatewayExe `
                -WorkingDirectory $RepoRoot `
                -RedirectStandardOutput $gwOut `
                -RedirectStandardError $gwErr `
                -PassThru `
                -NoNewWindow
        } finally {
            Pop-ProcessEnv -State $envState
        }

        Start-Sleep -Seconds ([int][Math]::Max(1, $WarmupSeconds))
        if ($gatewayProc.HasExited) {
            throw "gateway exited early"
        }
    }

    $gatewayUrl = ("http://{0}" -f $GatewayBind)
    if ($SeedFromCanarySummary) {
        $summary.seed_result = Import-SeedCandidates -GatewayUrl $gatewayUrl -ChainId $ChainId -SummaryPath $SeedSummaryPath -SeedMax $SeedMaxImport
    }

    $ticksFromDuration = [Math]::Ceiling(([double][UInt64]$DurationMinutes * 60.0) / [double][UInt64][Math]::Max(1, $IntervalSeconds))
    $ticks = [int][Math]::Max(1, $ticksFromDuration)
    $prevPending = $null
    $connectivityGuardFailure = $null
    for ($i = 1; $i -le $ticks; $i++) {
        try {
            $row = Measure-UniswapState -GatewayUrl $gatewayUrl -ChainId $ChainId -SampleMax $SampleMax
            $pendingNow = [Int64]$row.txpool_pending
            $pendingDelta = if ($null -eq $prevPending) { [Int64]0 } else { [Int64]($pendingNow - [Int64]$prevPending) }
            $row.pending_delta = $pendingDelta
            $row.uniswap_ratio_pct = if ([UInt64]$row.txpool_pending -gt 0) {
                [Math]::Round((100.0 * [double][UInt64]$row.uniswap_total) / [double][UInt64]$row.txpool_pending, 2)
            } else {
                [double]0
            }
            $prevPending = [UInt64]$row.txpool_pending

            $summary.observations += $row
            if ([UInt64]$row.peer_count -gt [UInt64]$summary.aggregate.max_peer_count) { $summary.aggregate.max_peer_count = [UInt64]$row.peer_count }
            if ([UInt64]$row.txpool_pending -gt [UInt64]$summary.aggregate.max_txpool_pending) { $summary.aggregate.max_txpool_pending = [UInt64]$row.txpool_pending }
            if ([UInt64]$row.pending_ingress_count -gt [UInt64]$summary.aggregate.max_pending_ingress) { $summary.aggregate.max_pending_ingress = [UInt64]$row.pending_ingress_count }
            if ([UInt64]$row.uniswap_v2 -gt [UInt64]$summary.aggregate.max_uniswap_v2) { $summary.aggregate.max_uniswap_v2 = [UInt64]$row.uniswap_v2 }
            if ([UInt64]$row.uniswap_v3 -gt [UInt64]$summary.aggregate.max_uniswap_v3) { $summary.aggregate.max_uniswap_v3 = [UInt64]$row.uniswap_v3 }
            if ([UInt64]$row.uniswap_total -gt [UInt64]$summary.aggregate.max_uniswap_total) { $summary.aggregate.max_uniswap_total = [UInt64]$row.uniswap_total }
            if ([UInt64]$row.other_pending -gt [UInt64]$summary.aggregate.max_other_pending) { $summary.aggregate.max_other_pending = [UInt64]$row.other_pending }
            if ([Int64]$row.pending_delta -gt [Int64]$summary.aggregate.max_pending_delta) { $summary.aggregate.max_pending_delta = [Int64]$row.pending_delta }
            if ([Int64]$row.pending_delta -lt [Int64]$summary.aggregate.min_pending_delta) { $summary.aggregate.min_pending_delta = [Int64]$row.pending_delta }
            if ([UInt64]$row.ingest_evicted_total -gt [UInt64]$summary.aggregate.max_ingest_evicted_total) { $summary.aggregate.max_ingest_evicted_total = [UInt64]$row.ingest_evicted_total }
            if ([UInt64]$row.ingest_evicted_last_tick -gt [UInt64]$summary.aggregate.max_ingest_evicted_last_tick) { $summary.aggregate.max_ingest_evicted_last_tick = [UInt64]$row.ingest_evicted_last_tick }
            if ([UInt64]$row.plugin_reachable -gt [UInt64]$summary.aggregate.max_plugin_reachable) { $summary.aggregate.max_plugin_reachable = [UInt64]$row.plugin_reachable }
            if ([UInt64]$row.plugin_tier_core_items -gt [UInt64]$summary.aggregate.max_plugin_tier_core_items) { $summary.aggregate.max_plugin_tier_core_items = [UInt64]$row.plugin_tier_core_items }
            if ([UInt64]$row.plugin_tier_active_items -gt [UInt64]$summary.aggregate.max_plugin_tier_active_items) { $summary.aggregate.max_plugin_tier_active_items = [UInt64]$row.plugin_tier_active_items }
            if ([UInt64]$row.plugin_tier_candidate_items -gt [UInt64]$summary.aggregate.max_plugin_tier_candidate_items) { $summary.aggregate.max_plugin_tier_candidate_items = [UInt64]$row.plugin_tier_candidate_items }
            $topPeerLabel = ""
            if ($null -ne $row.plugin_peer_top -and $row.plugin_peer_top.Count -gt 0) {
                $top = $row.plugin_peer_top[0]
                $topPeerLabel = ("{0}|{1}|s={2}|h={3}|u={4}|swap={5}/{6}|lat={7}ms|p={8}|d={9}" -f `
                    [string]$top.addr_hint, `
                    [string]$top.tier, `
                    [Int64]$top.score, `
                    [UInt64]$top.new_pooled_hashes_total, `
                    [UInt64]$top.unique_new_pooled_hashes_total, `
                    [UInt64](Get-ObjectFieldOrDefault -Object $top -Name "swap_hits_total" -Default 0), `
                    [UInt64](Get-ObjectFieldOrDefault -Object $top -Name "recent_swap_hits_total" -Default 0), `
                    [UInt64](Get-ObjectFieldOrDefault -Object $top -Name "avg_first_swap_latency_ms" -Default 0), `
                    [UInt64]$top.pooled_txs_total, `
                    [UInt64]$top.disconnect_count)
            } else {
                $topPeerLabel = "n/a"
            }

            Write-Host ("[{0}/{1}] peer={2} pending={3} dPending={4} other={5} ingress={6} uniV2={7} uniV3={8} uniTotal={9} uniPct={10}% import={11} evict={12}(confirm={13},stale={14}) pluginReachable={15} ready={16} auth={17} disc={18} tier(core/active/cand)={19}/{20}/{21} topPeer={22}" -f `
                $i, $ticks, $row.peer_count, $row.txpool_pending, $row.pending_delta, $row.other_pending, $row.pending_ingress_count, `
                $row.uniswap_v2, $row.uniswap_v3, $row.uniswap_total, $row.uniswap_ratio_pct, `
                $row.ingest_imported_last_tick, `
                $row.ingest_evicted_last_tick, $row.ingest_evicted_confirmed_last_tick, $row.ingest_evicted_stale_last_tick, $row.plugin_reachable, $row.plugin_session_ready, $row.plugin_auth_sent_items, $row.plugin_disconnected_items, $row.plugin_tier_core_items, $row.plugin_tier_active_items, $row.plugin_tier_candidate_items, $topPeerLabel)
            if (-not [string]::IsNullOrWhiteSpace([string]$row.ingest_last_error)) {
                Write-Host ("  ingestLastError={0}" -f $row.ingest_last_error)
            }
            if (-not [string]::IsNullOrWhiteSpace([string]$row.plugin_last_error)) {
                Write-Host ("  pluginLastError={0}" -f $row.plugin_last_error)
            }

            if ([UInt64]$row.peer_count -eq 0 -and [UInt64]$row.plugin_reachable -eq 0) {
                $connectivityGuardFailure = ("public connectivity unavailable at tick {0}/{1}: peer=0 and pluginReachable=0" -f $i, $ticks)
                $summary.connectivity_guard_error = $connectivityGuardFailure
                break
            }
        } catch {
            $summary.aggregate.error_ticks = [UInt64]$summary.aggregate.error_ticks + 1
            $summary.observations += [ordered]@{
                observed_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
                error = $_.Exception.Message
            }
            Write-Warning ("tick {0}/{1} failed: {2}" -f $i, $ticks, $_.Exception.Message)
        }

        if ($i -lt $ticks) {
            Start-Sleep -Seconds ([int][Math]::Max(1, $IntervalSeconds))
        }
    }
    if ($null -ne $connectivityGuardFailure) {
        throw $connectivityGuardFailure
    }
}
finally {
    $smokeFailureMessage = $null
    if ($null -ne $gatewayProc -and -not $gatewayProc.HasExited) {
        try {
            Stop-Process -Id $gatewayProc.Id -Force -ErrorAction SilentlyContinue
        } catch {
        }
    }
    $stageNames = @(
        "hello_received",
        "status_received",
        "status_sent",
        "ready",
        "new_pooled_hashes",
        "get_pooled_sent",
        "pooled_txs"
    )
    $stageCounts = [ordered]@{}
    foreach ($name in $stageNames) {
        $stageCounts[$name] = [UInt64]0
    }
    $firstPostReadyFrameCode = $null
    if (Test-Path $gwErr) {
        try {
            foreach ($line in Get-Content $gwErr) {
                if ($null -eq $firstPostReadyFrameCode -and $line -like "*gateway_warn: rlpx stage first_post_ready_frame*") {
                    $match = [regex]::Match($line, "code=(0x[0-9a-fA-F]+)")
                    if ($match.Success) {
                        $firstPostReadyFrameCode = $match.Groups[1].Value.ToLowerInvariant()
                    }
                }
                foreach ($name in $stageNames) {
                    if ($line -like ("*gateway_warn: rlpx stage {0}*" -f $name)) {
                        $stageCounts[$name] = [UInt64]$stageCounts[$name] + 1
                    }
                }
            }
        } catch {
        }
    }
    $fallbackReady = [UInt64]0
    $fallbackNewHashes = [UInt64]0
    $fallbackUniqueNewHashes = [UInt64]0
    $fallbackDuplicateNewHashes = [UInt64]0
    $fallbackGetPooled = [UInt64]0
    $fallbackPooled = [UInt64]0
    $fallbackUniquePooled = [UInt64]0
    $fallbackDuplicatePooled = [UInt64]0
    $fallbackFirstSeenHashes = [UInt64]0
    $fallbackFirstSeenTxs = [UInt64]0
    foreach ($row in @($summary.observations)) {
        if ($null -eq $row) {
            continue
        }
        [UInt64]$value = 0
        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_ready_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackReady) { $fallbackReady = $value }

        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_new_hashes_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackNewHashes) { $fallbackNewHashes = $value }

        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_unique_new_hashes_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackUniqueNewHashes) { $fallbackUniqueNewHashes = $value }

        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_duplicate_new_hashes_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackDuplicateNewHashes) { $fallbackDuplicateNewHashes = $value }

        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_get_pooled_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackGetPooled) { $fallbackGetPooled = $value }

        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_pooled_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackPooled) { $fallbackPooled = $value }

        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_unique_pooled_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackUniquePooled) { $fallbackUniquePooled = $value }

        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_duplicate_pooled_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackDuplicatePooled) { $fallbackDuplicatePooled = $value }

        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_first_seen_hash_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackFirstSeenHashes) { $fallbackFirstSeenHashes = $value }

        [void][UInt64]::TryParse([string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_first_seen_tx_total" -Default "0"), [ref]$value)
        if ($value -gt $fallbackFirstSeenTxs) { $fallbackFirstSeenTxs = $value }

        if ($null -eq $firstPostReadyFrameCode) {
            $firstCodeRaw = [string](Get-ObjectFieldOrDefault -Object $row -Name "plugin_worker_first_post_ready_code" -Default "")
            if (-not [string]::IsNullOrWhiteSpace($firstCodeRaw) -and $firstCodeRaw -ne "0x0") {
                $firstPostReadyFrameCode = $firstCodeRaw.ToLowerInvariant()
            }
        }
    }
    if ([UInt64]$stageCounts["ready"] -eq 0 -and $fallbackReady -gt 0) {
        $stageCounts["ready"] = $fallbackReady
    }
    if ([UInt64]$stageCounts["new_pooled_hashes"] -eq 0 -and $fallbackNewHashes -gt 0) {
        $stageCounts["new_pooled_hashes"] = $fallbackNewHashes
    }
    if ([UInt64]$stageCounts["get_pooled_sent"] -eq 0 -and $fallbackGetPooled -gt 0) {
        $stageCounts["get_pooled_sent"] = $fallbackGetPooled
    }
    if ([UInt64]$stageCounts["pooled_txs"] -eq 0 -and $fallbackPooled -gt 0) {
        $stageCounts["pooled_txs"] = $fallbackPooled
    }
    $summary.rlpx_stage_event_counts = $stageCounts
    Write-Host ("rlpx stage summary: hello={0} statusR={1} statusS={2} ready={3} newHash={4} getPooled={5} pooled={6}" -f `
        $stageCounts["hello_received"], `
        $stageCounts["status_received"], `
        $stageCounts["status_sent"], `
        $stageCounts["ready"], `
        $stageCounts["new_pooled_hashes"], `
        $stageCounts["get_pooled_sent"], `
        $stageCounts["pooled_txs"])
    $smokeReasons = New-Object System.Collections.Generic.List[string]
    $smokePassed = $true
    if ([UInt64]$stageCounts["ready"] -lt [UInt64][Math]::Max(0, $SmokeMinReady)) {
        $smokePassed = $false
        [void]$smokeReasons.Add(("ready<{0}" -f [UInt64][Math]::Max(0, $SmokeMinReady)))
    }
    if ([UInt64]$stageCounts["new_pooled_hashes"] -lt [UInt64][Math]::Max(0, $SmokeMinNewPooled)) {
        $smokePassed = $false
        [void]$smokeReasons.Add(("new_pooled_hashes<{0}" -f [UInt64][Math]::Max(0, $SmokeMinNewPooled)))
    }
    if ([UInt64]$stageCounts["pooled_txs"] -lt [UInt64][Math]::Max(0, $SmokeMinPooled)) {
        $smokePassed = $false
        [void]$smokeReasons.Add(("pooled_txs<{0}" -f [UInt64][Math]::Max(0, $SmokeMinPooled)))
    }
    if ($SmokeRequireFirstFrameNewPooled) {
        if ([string]::IsNullOrWhiteSpace([string]$firstPostReadyFrameCode)) {
            $smokePassed = $false
            [void]$smokeReasons.Add("first_post_ready_frame_missing")
        } elseif ($firstPostReadyFrameCode -ne "0x18") {
            $smokePassed = $false
            [void]$smokeReasons.Add(("first_post_ready_frame_code={0}" -f $firstPostReadyFrameCode))
        }
    }
    $summary.smoke = [ordered]@{
        passed = [bool]$smokePassed
        reasons = @($smokeReasons)
        first_post_ready_frame_code = $firstPostReadyFrameCode
        observed_ready = [UInt64]$stageCounts["ready"]
        observed_new_pooled = [UInt64]$stageCounts["new_pooled_hashes"]
        observed_pooled = [UInt64]$stageCounts["pooled_txs"]
    }
    $peerQualityTop = @(Build-PeerQualityTop -Observations $summary.observations -TopN 10)
    $peerTopStability = Build-PeerTopStabilityStats -Observations $summary.observations
    $peerContribution = Build-PeerContributionStats -PeerQualityTop $peerQualityTop
    $coreUniqueDistribution = @(Build-CoreUniqueDistribution -PeerQualityTop $peerQualityTop -TopN 8)
    [UInt64]$peerTierCore = 0
    [UInt64]$peerTierActive = 0
    [UInt64]$peerTierCandidate = 0
    foreach ($item in $peerQualityTop) {
        $tier = ([string]$item.tier).ToLowerInvariant()
        switch ($tier) {
            "core" { $peerTierCore++ }
            "active" { $peerTierActive++ }
            default { $peerTierCandidate++ }
        }
    }
    $summary.peer_quality_top = @($peerQualityTop)
    $summary.peer_top_stability = $peerTopStability
    $summary.peer_contribution = $peerContribution
    $summary.core_unique_distribution = @($coreUniqueDistribution)
    $summary.unique_total = [UInt64]$fallbackUniqueNewHashes
    $summary.duplicate_total = [UInt64]$fallbackDuplicateNewHashes
    $summary.first_seen_total = [UInt64]$fallbackFirstSeenHashes
    $summary.top1_unique_share = [double]$peerContribution.top1_unique_hash_share_pct
    $summary.top3_unique_share = [double]$peerContribution.top3_unique_hash_share_pct
    $summary.top5_unique_share = [double]$peerContribution.top5_unique_hash_share_pct
    $summary.peer_tier_counts = [ordered]@{
        core = [UInt64]$summary.aggregate.max_plugin_tier_core_items
        active = [UInt64]$summary.aggregate.max_plugin_tier_active_items
        candidate = [UInt64]$summary.aggregate.max_plugin_tier_candidate_items
    }
    $summary.peer_tier_counts_top = [ordered]@{
        core = [UInt64]$peerTierCore
        active = [UInt64]$peerTierActive
        candidate = [UInt64]$peerTierCandidate
    }
    Write-Host ("peer top1 stability: ticks={0} coreHit={1}% activeHit={2}% candHit={3}% switches={4} coreStreak={5}" -f `
        [UInt64]$peerTopStability.ticks, `
        [double]$peerTopStability.top1_core_hit_rate_pct, `
        [double]$peerTopStability.top1_active_hit_rate_pct, `
        [double]$peerTopStability.top1_candidate_hit_rate_pct, `
        [UInt64]$peerTopStability.top1_peer_switches, `
        [UInt64]$peerTopStability.top1_core_max_streak)
    Write-Host ("peer contribution: top1={0}% top3={1}% top5={2}% totalHashes={3}" -f `
        [double]$peerContribution.top1_hash_share_pct, `
        [double]$peerContribution.top3_hash_share_pct, `
        [double]$peerContribution.top5_hash_share_pct, `
        [UInt64]$peerContribution.total_hashes)
    Write-Host ("peer unique contribution: top1={0}% top3={1}% top5={2}% uniqueHashes={3} dupHashes={4} firstSeen={5}" -f `
        [double]$peerContribution.top1_unique_hash_share_pct, `
        [double]$peerContribution.top3_unique_hash_share_pct, `
        [double]$peerContribution.top5_unique_hash_share_pct, `
        [UInt64]$peerContribution.total_unique_hashes, `
        [UInt64]$peerContribution.total_duplicate_hashes, `
        [UInt64]$peerContribution.total_first_seen_hashes)
    if (@($peerQualityTop).Count -gt 0) {
        Write-Host "peer quality top:"
        foreach ($item in $peerQualityTop) {
            Write-Host ("  {0}|{1}|score={2}|hashes={3}|uHash={4}|dHash={5}|swap={6}(u={7},v2={8},v3={9},recent={10}/{11})|lat={12}/{13}ms|firstSeen={14}|pooled={15}|disc={16}|hits={17}" -f `
                [string]$item.addr_hint, `
                [string]$item.tier, `
                [Int64]$item.best_score, `
                [UInt64]$item.max_new_pooled_hashes_total, `
                [UInt64]$item.max_unique_new_pooled_hashes_total, `
                [UInt64]$item.max_duplicate_new_pooled_hashes_total, `
                [UInt64]$item.max_swap_hits_total, `
                [UInt64]$item.max_unique_swap_hits_total, `
                [UInt64]$item.max_swap_v2_hits_total, `
                [UInt64]$item.max_swap_v3_hits_total, `
                [UInt64]$item.max_recent_swap_hits_total, `
                [UInt64]$item.max_recent_unique_swap_hits_total, `
                [UInt64]$item.min_avg_first_gossip_latency_ms, `
                [UInt64]$item.min_avg_first_swap_latency_ms, `
                [UInt64]$item.max_first_seen_hash_count, `
                [UInt64]$item.max_pooled_txs_total, `
                [UInt64]$item.max_disconnect_count, `
                [UInt64]$item.top_hits)
        }
    } else {
        Write-Host "peer quality top: none"
    }
    if ($coreUniqueDistribution.Count -gt 0) {
        Write-Host "core unique distribution:"
        foreach ($item in $coreUniqueDistribution) {
            Write-Host ("  {0}|score={1}|uHash={2}|dHash={3}|firstSeen={4}|dupPct={5}%" -f `
                [string]$item.addr_hint, `
                [Int64]$item.score, `
                [UInt64]$item.unique_hashes, `
                [UInt64]$item.duplicate_hashes, `
                [UInt64]$item.first_seen_hashes, `
                [double]$item.duplicate_ratio_pct)
        }
    } else {
        Write-Host "core unique distribution: none"
    }
    Write-Host ("smoke summary: pass={0} firstCode={1} reasons={2}" -f `
        $smokePassed, `
        $(if ($null -eq $firstPostReadyFrameCode) { "n/a" } else { $firstPostReadyFrameCode }), `
        $(if ($smokeReasons.Count -eq 0) { "none" } else { ($smokeReasons -join ",") }))
    if ($SmokeAssert.IsPresent -and -not $smokePassed) {
        $smokeFailureMessage = ("smoke assert failed: {0}" -f ($smokeReasons -join ","))
    }
    $summary.completed_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    $summaryJson = $summary | ConvertTo-Json -Depth 100
    Set-Content -Path $SummaryOut -Value $summaryJson -Encoding UTF8
    Write-Host ("summary written: {0}" -f $SummaryOut)
    if ($null -ne $smokeFailureMessage) {
        throw $smokeFailureMessage
    }
}
