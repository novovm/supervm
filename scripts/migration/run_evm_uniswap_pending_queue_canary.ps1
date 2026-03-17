param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [UInt64]$ChainId = 1,
    [string]$SourceRpc = "",
    [UInt64]$MaxImport = 5,
    [UInt64]$GatewayWarmupMs = 1800,
    [string]$SummaryOut = "artifacts/migration/evm-uniswap-pending-queue-canary-summary.json",
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($SourceRpc)) {
    throw "SourceRpc is required for canary seeding (synthetic import path)."
}

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

function Invoke-JsonRpc {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params,
        [UInt64]$TimeoutSec = 30
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
        throw ("{0} failed: missing result" -f $Method)
    }
    return $resp.result
}

function Convert-AnyToUInt64 {
    param($Value)
    if ($null -eq $Value) {
        return $null
    }
    if ($Value -is [UInt64]) {
        return [UInt64]$Value
    }
    if ($Value -is [Int64]) {
        if ([Int64]$Value -lt 0) {
            return $null
        }
        return [UInt64]$Value
    }
    $raw = [string]$Value
    $trimmed = $raw.Trim()
    if ([string]::IsNullOrWhiteSpace($trimmed)) {
        return $null
    }
    if ($trimmed.StartsWith("0x", [System.StringComparison]::OrdinalIgnoreCase)) {
        $hex = $trimmed.Substring(2)
        if ([string]::IsNullOrWhiteSpace($hex)) {
            return $null
        }
        try {
            return [Convert]::ToUInt64($hex, 16)
        } catch {
            return $null
        }
    }
    try {
        return [UInt64]::Parse($trimmed)
    } catch {
        return $null
    }
}

function Ensure-GatewayBindingOwner {
    param(
        [string]$GatewayUrl,
        [UInt64]$ChainId,
        [string]$FromAddress,
        [string]$PreferredUca
    )
    try {
        $lookup = Invoke-JsonRpc -Url $GatewayUrl -Method "ua_getBindingOwner" -Params @{
            persona_type = "evm"
            chain_id = [UInt64]$ChainId
            external_address = $FromAddress
        } -TimeoutSec 20
        if ($null -ne $lookup -and [bool]$lookup.found -and -not [string]::IsNullOrWhiteSpace([string]$lookup.owner_uca_id)) {
            return [string]$lookup.owner_uca_id
        }
    } catch {
        # continue with create+bind path
    }

    try {
        Invoke-JsonRpc -Url $GatewayUrl -Method "ua_createUca" -Params @{ uca_id = $PreferredUca } -TimeoutSec 20 | Out-Null
    } catch {
        # idempotent: ignore
    }

    try {
        Invoke-JsonRpc -Url $GatewayUrl -Method "ua_bindPersona" -Params @{
            uca_id = $PreferredUca
            persona_type = "evm"
            chain_id = [UInt64]$ChainId
            external_address = $FromAddress
        } -TimeoutSec 20 | Out-Null
        return $PreferredUca
    } catch {
        $msg = $_.Exception.Message
        $patterns = @(
            "binding_owner=([a-zA-Z0-9_\\-]+)",
            "existing_owner=([a-zA-Z0-9_\\-]+)",
            "existing owner\\s*:\\s*([a-zA-Z0-9_\\-]+)",
            "owner_uca_id\\s*[=:]\\s*([a-zA-Z0-9_\\-]+)"
        )
        foreach ($pattern in $patterns) {
            $m = [regex]::Match($msg, $pattern, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
            if ($m.Success) {
                return $m.Groups[1].Value
            }
        }
        throw
    }
}

function Get-UniswapPendingCandidates {
    param(
        [string]$SourceRpc,
        [UInt64]$MaxImport
    )
    $routers = @(
        "0x7a250d5630b4cf539739df2c5dacb4c659f2488d", # V2 Router
        "0xef1c6e67703c7bd7107eed8303fbe6ec2554bf6b", # Universal Router
        "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45", # SwapRouter02
        "0xe592427a0aece92de3edee1f18e0157c05861564"  # V3 SwapRouter
    )
    $routerSet = New-Object "System.Collections.Generic.HashSet[string]" ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($router in $routers) {
        $null = $routerSet.Add($router)
    }

    $content = Invoke-JsonRpc -Url $SourceRpc -Method "txpool_content" -Params @() -TimeoutSec 40
    $bestBySender = @{}

    $pendingRoot = $content.pending
    if ($null -eq $pendingRoot) {
        return @()
    }

    foreach ($fromProp in $pendingRoot.PSObject.Properties) {
        $byNonce = $fromProp.Value
        if ($null -eq $byNonce) {
            continue
        }
        foreach ($nonceProp in $byNonce.PSObject.Properties) {
            $tx = $nonceProp.Value
            if ($null -eq $tx) {
                continue
            }
            $to = [string]$tx.to
            if ([string]::IsNullOrWhiteSpace($to)) {
                continue
            }
            $toLower = $to.ToLowerInvariant()
            if (-not $routerSet.Contains($toLower)) {
                continue
            }
            $from = [string]$tx.from
            if ([string]::IsNullOrWhiteSpace($from)) {
                continue
            }
            $fromLower = $from.ToLowerInvariant()
            $nonceValue = Convert-AnyToUInt64 -Value $tx.nonce
            if ($null -eq $nonceValue) {
                continue
            }
            $hash = [string]$tx.hash
            if ([string]::IsNullOrWhiteSpace($hash)) {
                continue
            }
            $candidate = [pscustomobject]@{
                hash = $hash
                from = $from
                to = $to
                nonce_u64 = [UInt64]$nonceValue
                nonce = [string]$tx.nonce
                gas_price = [string]$tx.gasPrice
            }
            if (-not $bestBySender.ContainsKey($fromLower)) {
                $bestBySender[$fromLower] = $candidate
                continue
            }
            $existing = $bestBySender[$fromLower]
            if ([UInt64]$candidate.nonce_u64 -lt [UInt64]$existing.nonce_u64) {
                $bestBySender[$fromLower] = $candidate
            }
        }
    }

    if ($bestBySender.Count -eq 0) {
        return @()
    }

    $selected = $bestBySender.Values | Sort-Object `
        @{Expression = { if ([UInt64]$_.nonce_u64 -eq 0) { 0 } else { 1 } }; Ascending = $true }, `
        @{Expression = { [UInt64]$_.nonce_u64 }; Ascending = $true }, `
        @{Expression = { [string]$_.from }; Ascending = $true }

    $found = New-Object System.Collections.Generic.List[object]
    foreach ($candidate in $selected) {
        if ($found.Count -ge [int]$MaxImport) {
            break
        }
        $raw = Invoke-JsonRpc -Url $SourceRpc -Method "eth_getRawTransactionByHash" -Params @([string]$candidate.hash) -TimeoutSec 25
        if ($null -eq $raw) {
            continue
        }
        $rawStr = [string]$raw
        if (-not $rawStr.StartsWith("0x") -or $rawStr.Length -lt 4) {
            continue
        }
        $found.Add([pscustomobject]@{
            hash = [string]$candidate.hash
            from = [string]$candidate.from
            to = [string]$candidate.to
            raw_tx = $rawStr
            nonce = [string]$candidate.nonce
            nonce_u64 = [UInt64]$candidate.nonce_u64
            gas_price = [string]$candidate.gas_price
        })
    }
    return $found.ToArray()
}

function Count-UniswapPendingInGateway {
    param(
        [string]$GatewayUrl
    )
    $routers = @(
        "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
        "0xef1c6e67703c7bd7107eed8303fbe6ec2554bf6b",
        "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45",
        "0xe592427a0aece92de3edee1f18e0157c05861564"
    )
    $routerSet = New-Object "System.Collections.Generic.HashSet[string]" ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($router in $routers) {
        $null = $routerSet.Add($router)
    }

    $content = Invoke-JsonRpc -Url $GatewayUrl -Method "txpool_content" -Params @{ chain_id = 1 } -TimeoutSec 20
    $count = 0
    $sample = @()
    if ($null -ne $content.pending) {
        foreach ($fromProp in $content.pending.PSObject.Properties) {
            foreach ($nonceProp in $fromProp.Value.PSObject.Properties) {
                $tx = $nonceProp.Value
                $to = [string]$tx.to
                if ([string]::IsNullOrWhiteSpace($to)) {
                    continue
                }
                if (-not $routerSet.Contains($to.ToLowerInvariant())) {
                    continue
                }
                $count += 1
                if ($sample.Count -lt 8) {
                    $sample += [pscustomobject]@{
                        from = [string]$tx.from
                        to = $to
                        nonce = [string]$tx.nonce
                        hash = [string]$tx.hash
                    }
                }
            }
        }
    }
    return [pscustomobject]@{
        count = $count
        sample = $sample
        txpool_content = $content
    }
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$SummaryDir = Split-Path -Parent $SummaryOut
if ($SummaryDir) {
    New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
}

if (-not $SkipBuild) {
    & cargo build -p novovm-evm-gateway
    if ($LASTEXITCODE -ne 0) {
        throw "build failed: novovm-evm-gateway"
    }
}

$gatewayExe = Join-Path $RepoRoot "target\debug\novovm-evm-gateway.exe"
if (-not (Test-Path $gatewayExe)) {
    throw "gateway binary not found: $gatewayExe"
}

$logDir = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$gwOut = Join-Path $logDir "evm-uniswap-pending-queue-canary-gateway.stdout.log"
$gwErr = Join-Path $logDir "evm-uniswap-pending-queue-canary-gateway.stderr.log"
if (Test-Path $gwOut) { Remove-Item -Force $gwOut }
if (Test-Path $gwErr) { Remove-Item -Force $gwErr }
$runTag = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$runTmpDir = Resolve-FullPath -Root $RepoRoot -Value ("artifacts/migration/tmp/evm-uniswap-canary-{0}" -f $runTag)
New-Item -ItemType Directory -Force -Path $runTmpDir | Out-Null
$gatewayUaStorePath = Join-Path $runTmpDir "gateway-ua-store.bin"
$gatewaySpoolDir = Join-Path $runTmpDir "gateway-spool"
New-Item -ItemType Directory -Force -Path $gatewaySpoolDir | Out-Null

$summary = [ordered]@{
    started_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    gateway_bind = $GatewayBind
    chain_id = $ChainId
    source_rpc = $SourceRpc
    max_import = $MaxImport
    source_candidates = @()
    imported_attempted = 0
    imported_ok = 0
    imported_failed = 0
    import_errors = @()
    local_uniswap_pending_count = 0
    local_uniswap_pending_sample = @()
    pass = $false
    gateway_stdout = $gwOut
    gateway_stderr = $gwErr
    run_tmp_dir = $runTmpDir
    gateway_ua_store_path = $gatewayUaStorePath
    gateway_spool_dir = $gatewaySpoolDir
}

$gatewayProc = $null
try {
    $candidates = @(Get-UniswapPendingCandidates -SourceRpc $SourceRpc -MaxImport $MaxImport)
    $summary.source_candidates = $candidates
    if (@($candidates).Count -eq 0) {
        throw "no uniswap pending candidates found on source rpc"
    }

    $envMap = @{
        "NOVOVM_GATEWAY_BIND" = $GatewayBind
        "NOVOVM_GATEWAY_UA_STORE_BACKEND" = "bincode_file"
        "NOVOVM_GATEWAY_UA_STORE_PATH" = $gatewayUaStorePath
        "NOVOVM_GATEWAY_SPOOL_DIR" = $gatewaySpoolDir
        "NOVOVM_GATEWAY_ETH_TX_INDEX_BACKEND" = "memory"
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_EXEC" = ""
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC" = ""
        "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC" = ""
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_REQUIRED" = "0"
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES" = "0"
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_BOOTNODES" = ""
        "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS" = ""
    }
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_REQUIRED_CHAIN_$ChainId"] = "0"
    $chainIdHex = ("0x{0:x}" -f [UInt64]$ChainId)
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_REQUIRED_CHAIN_$chainIdHex"] = "0"
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES_CHAIN_$ChainId"] = "0"
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_ENABLE_BUILTIN_BOOTNODES_CHAIN_$chainIdHex"] = "0"
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_BOOTNODES_CHAIN_$ChainId"] = ""
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_BOOTNODES_CHAIN_$chainIdHex"] = ""
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS_CHAIN_$ChainId"] = ""
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS_CHAIN_$chainIdHex"] = ""
    $envBackup = @{}
    foreach ($entry in $envMap.GetEnumerator()) {
        $name = [string]$entry.Key
        $envBackup[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
        [Environment]::SetEnvironmentVariable($name, [string]$entry.Value, "Process")
    }
    try {
        $gatewayProc = Start-Process `
            -FilePath $gatewayExe `
            -WorkingDirectory $RepoRoot `
            -RedirectStandardOutput $gwOut `
            -RedirectStandardError $gwErr `
            -PassThru `
            -NoNewWindow
    }
    finally {
        foreach ($entry in $envMap.GetEnumerator()) {
            $name = [string]$entry.Key
            $old = $null
            if ($envBackup.ContainsKey($name)) {
                $old = $envBackup[$name]
            }
            [Environment]::SetEnvironmentVariable($name, $old, "Process")
        }
    }

    Start-Sleep -Milliseconds ([int][Math]::Max(500, $GatewayWarmupMs))
    if ($gatewayProc.HasExited) {
        throw "gateway exited early"
    }
    $gatewayUrl = "http://$GatewayBind"

    $i = 0
    $senderOwnerMap = @{}
    foreach ($candidate in $candidates) {
        $i += 1
        $summary.imported_attempted += 1
        $from = [string]$candidate.from
        $rawTx = [string]$candidate.raw_tx
        try {
            $fromKey = $from.ToLowerInvariant()
            $owner = $null
            if ($senderOwnerMap.ContainsKey($fromKey)) {
                $owner = [string]$senderOwnerMap[$fromKey]
            } else {
                $ucaSeed = $fromKey.Replace("0x", "")
                $ucaId = ("uca-uniswap-pending-{0}" -f $ucaSeed)
                $owner = Ensure-GatewayBindingOwner -GatewayUrl $gatewayUrl -ChainId $ChainId -FromAddress $from -PreferredUca $ucaId
                $senderOwnerMap[$fromKey] = $owner
            }
            $sendResult = Invoke-JsonRpc -Url $gatewayUrl -Method "eth_sendRawTransaction" -Params @{
                uca_id = $owner
                chain_id = [UInt64]$ChainId
                from = $from
                raw_tx = $rawTx
                require_public_broadcast = $false
                return_detail = $true
            } -TimeoutSec 30
            $ok = $false
            if ($sendResult -is [string]) {
                $ok = $sendResult.StartsWith("0x")
            } elseif ($null -ne $sendResult.accepted) {
                $ok = [bool]$sendResult.accepted
            } elseif ($null -ne $sendResult.tx_hash) {
                $ok = [string]$sendResult.tx_hash -like "0x*"
            }
            if ($ok) {
                $summary.imported_ok += 1
            } else {
                $summary.imported_failed += 1
                $summary.import_errors += ("send returned non-accepted for hash={0}" -f [string]$candidate.hash)
            }
        } catch {
            $summary.imported_failed += 1
            $summary.import_errors += ("hash={0} err={1}" -f [string]$candidate.hash, $_.Exception.Message)
        }
    }

    $local = Count-UniswapPendingInGateway -GatewayUrl $gatewayUrl
    $summary.local_uniswap_pending_count = [int]$local.count
    $summary.local_uniswap_pending_sample = $local.sample
    $summary.local_txpool_content = $local.txpool_content
    $summary.pass = ($summary.local_uniswap_pending_count -gt 0)
    if (-not $summary.pass) {
        throw "no uniswap pending tx visible in local gateway txpool"
    }
}
finally {
    if ($null -ne $gatewayProc -and -not $gatewayProc.HasExited) {
        try {
            Stop-Process -Id $gatewayProc.Id -Force -ErrorAction SilentlyContinue
        } catch {
        }
    }
    $summary.completed_at_utc = [DateTimeOffset]::UtcNow.ToString("o")
    $summaryJson = $summary | ConvertTo-Json -Depth 80
    Set-Content -Path $SummaryOut -Value $summaryJson -Encoding UTF8
    Write-Host "summary written: $SummaryOut"
}

if (-not $summary.pass) {
    throw "evm uniswap pending queue canary failed; inspect summary: $SummaryOut"
}

Write-Host "evm uniswap pending queue canary ok"
