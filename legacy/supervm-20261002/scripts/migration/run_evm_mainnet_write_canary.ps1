param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [ValidateSet("fullnode_only", "upstream_proxy")]
    [string]$BroadcastMode = "fullnode_only",
    [string]$UpstreamRpc = "",
    [string]$RawTx = "",
    [string]$UcaId = "",
    [string]$FromAddress = "",
    [UInt64]$ChainId = 1,
    [int]$PollMaxAttempts = 45,
    [int]$PollIntervalMs = 3000,
    [UInt64]$UpstreamTimeoutMs = 20000,
    [switch]$RequireReceiptInFullnode,
    [switch]$DisableFullnodeAutoBootstrap,
    [string]$SummaryOut = "artifacts/migration/evm-mainnet-write-canary-summary.json",
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

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

function Resolve-TxHashFromResult {
    param(
        [Parameter(Mandatory = $true)]
        [AllowNull()]
        $Result,
        [string]$Context = "rpc result"
    )
    if ($null -eq $Result) {
        throw "$Context is null"
    }
    if ($Result -is [string]) {
        $raw = $Result.Trim()
        if (-not $raw) {
            throw "$Context tx hash string is empty"
        }
        return $raw
    }
    if ($Result -is [psobject]) {
        $txHashProp = $Result.PSObject.Properties["tx_hash"]
        if ($null -ne $txHashProp -and -not [string]::IsNullOrWhiteSpace([string]$txHashProp.Value)) {
            return ([string]$txHashProp.Value).Trim()
        }
        $hashProp = $Result.PSObject.Properties["hash"]
        if ($null -ne $hashProp -and -not [string]::IsNullOrWhiteSpace([string]$hashProp.Value)) {
            return ([string]$hashProp.Value).Trim()
        }
    }
    throw "$Context missing tx hash (expected string result, or object.tx_hash/hash)"
}

$RepoRoot = Resolve-RootPath -Root $RepoRoot
$SummaryOut = Resolve-FullPath -Root $RepoRoot -Value $SummaryOut
$SummaryDir = Split-Path -Parent $SummaryOut
if ($SummaryDir) {
    New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null
}

if (-not $UpstreamRpc) {
    $UpstreamRpc = $env:NOVOVM_GATEWAY_ETH_UPSTREAM_RPC
}
if (-not $RawTx) {
    $RawTx = $env:NOVOVM_EVM_MAINNET_CANARY_RAW_TX
}
if (-not $RawTx) {
    throw "missing RawTx: pass -RawTx or set NOVOVM_EVM_MAINNET_CANARY_RAW_TX"
}
if ($BroadcastMode -eq "upstream_proxy" -and -not $UpstreamRpc) {
    throw "missing UpstreamRpc in upstream_proxy mode: pass -UpstreamRpc or set NOVOVM_GATEWAY_ETH_UPSTREAM_RPC"
}

function Get-RpcField {
    param(
        [AllowNull()]$Obj,
        [Parameter(Mandatory = $true)][string]$Name
    )
    if ($null -eq $Obj) {
        return $null
    }
    $prop = $Obj.PSObject.Properties[$Name]
    if ($null -eq $prop) {
        return $null
    }
    return $prop.Value
}
if (-not $UcaId) {
    $UcaId = $env:NOVOVM_EVM_MAINNET_CANARY_UCA_ID
}

$TargetRoot = if ($env:CARGO_TARGET_DIR) {
    [System.IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
} else {
    Join-Path $RepoRoot "target"
}
if (-not $SkipBuild) {
    & cargo build -p novovm-evm-gateway
    if ($LASTEXITCODE -ne 0) {
        throw "build failed: novovm-evm-gateway"
    }
}
$GatewayExe = Resolve-BinaryPath -TargetRoot $TargetRoot -BinaryBaseName "novovm-evm-gateway"
if (-not (Test-Path $GatewayExe)) {
    throw "gateway binary not found: $GatewayExe"
}

$logDir = Resolve-FullPath -Root $RepoRoot -Value "artifacts/migration/logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$gwOut = Join-Path $logDir "evm-mainnet-canary-gateway.stdout.log"
$gwErr = Join-Path $logDir "evm-mainnet-canary-gateway.stderr.log"
if (Test-Path $gwOut) { Remove-Item -Force $gwOut }
if (Test-Path $gwErr) { Remove-Item -Force $gwErr }

$chainHex = ("0x{0:x}" -f $ChainId)
$envMap = @{
    "NOVOVM_GATEWAY_BIND" = $GatewayBind
    # Default strict full-node mode: clear upstream/proxy routes.
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC" = ""
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC" = ""
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_TIMEOUT_MS" = ""
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_TIMEOUT_MS" = ""
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_$ChainId" = ""
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_$chainHex" = ""
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_CHAIN_$ChainId" = ""
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_CHAIN_$chainHex" = ""
}

if ($BroadcastMode -eq "upstream_proxy") {
    $envMap["NOVOVM_GATEWAY_ETH_UPSTREAM_RPC"] = $UpstreamRpc
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC"] = $UpstreamRpc
    $envMap["NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_TIMEOUT_MS"] = ([string]$UpstreamTimeoutMs)
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_TIMEOUT_MS"] = ([string]$UpstreamTimeoutMs)
    $envMap["NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_$ChainId"] = $UpstreamRpc
    $envMap["NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_$chainHex"] = $UpstreamRpc
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_CHAIN_$ChainId"] = $UpstreamRpc
    $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_CHAIN_$chainHex"] = $UpstreamRpc
} elseif (-not $DisableFullnodeAutoBootstrap) {
    if (-not (Test-Path "Env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_NODE_ID")) {
        $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_NODE_ID"] = "1"
    }
    if (-not (Test-Path "Env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_LISTEN")) {
        $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_LISTEN"] = "127.0.0.1:39001"
    }
    if (-not (Test-Path "Env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_TRANSPORT")) {
        $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_TRANSPORT"] = "udp"
    }
    if (-not (Test-Path "Env:NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS")) {
        $envMap["NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_NATIVE_PEERS"] = "2@127.0.0.1:39001"
    }
}

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

$startedAt = [DateTimeOffset]::UtcNow
    $summary = [ordered]@{
    started_at_utc = $startedAt.ToString("o")
    gateway_bind = $GatewayBind
    broadcast_mode = $BroadcastMode
    chain_id = $ChainId
    upstream_rpc = $UpstreamRpc
        tx_hash = $null
        uca_id = $UcaId
        from_address = $FromAddress
    send_result = $null
    submit_status = $null
    runtime_protocol_caps = $null
    fullnode_native_ready = $null
    runtime_protocol_caps_after_send = $null
    fullnode_native_ready_after_send = $null
    accepted_or_pending = $false
    receipt = $null
    receipt_found = $false
    poll_attempts = 0
    gateway_stdout = $gwOut
    gateway_stderr = $gwErr
}

try {
    Start-Sleep -Milliseconds 900
    if ($gatewayProc.HasExited) {
        throw "gateway exited early"
    }

    $url = "http://$GatewayBind"
    $chainReq = @{
        jsonrpc = "2.0"
        id = 1
        method = "eth_chainId"
        params = @()
    } | ConvertTo-Json -Depth 6 -Compress
    $chainResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $chainReq
    $chainResult = Get-RpcField -Obj $chainResp -Name "result"
    if (-not $chainResult) {
        throw "eth_chainId returned empty result"
    }

    $capsReq = @{
        jsonrpc = "2.0"
        id = 9
        method = "evm_getRuntimeProtocolCaps"
        params = @{
            chain_id = [UInt64]$ChainId
        }
    } | ConvertTo-Json -Depth 10 -Compress
    $capsResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $capsReq
    $capsErr = Get-RpcField -Obj $capsResp -Name "error"
    if ($null -eq $capsErr) {
        $capsResult = Get-RpcField -Obj $capsResp -Name "result"
        if ($capsResult) {
            $summary.runtime_protocol_caps = $capsResult
            $nativeDiscovery = [bool](Get-RpcField -Obj $capsResult -Name "native_peer_discovery")
            $nativeHandshake = [bool](Get-RpcField -Obj $capsResult -Name "native_eth_handshake")
            $nativeSync = [bool](Get-RpcField -Obj $capsResult -Name "native_snap_sync_state_machine")
            $summary.fullnode_native_ready = ($nativeDiscovery -and $nativeHandshake -and $nativeSync)
        }
    }
    # In fullnode_only mode, readiness can be reached after first native send warmup.

    if ($UcaId -and $FromAddress) {
        $createReq = @{
            jsonrpc = "2.0"
            id = 11
            method = "ua_createUca"
            params = @{
                uca_id = $UcaId
            }
        } | ConvertTo-Json -Depth 8 -Compress
        $createResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $createReq
        $createErr = Get-RpcField -Obj $createResp -Name "error"
        if ($null -ne $createErr) {
            $createMsg = [string](Get-RpcField -Obj $createErr -Name "message")
            if ($createMsg -notmatch "(?i)uca.*exists|already exists") {
                $createCode = Get-RpcField -Obj $createErr -Name "code"
                throw ("ua_createUca failed: code={0} message={1}" -f $createCode, $createMsg)
            }
        }

        $bindReq = @{
            jsonrpc = "2.0"
            id = 12
            method = "ua_bindPersona"
            params = @{
                uca_id = $UcaId
                persona_type = "evm"
                chain_id = [UInt64]$ChainId
                external_address = $FromAddress
            }
        } | ConvertTo-Json -Depth 10 -Compress
        $bindResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $bindReq
        $bindErr = Get-RpcField -Obj $bindResp -Name "error"
        if ($null -ne $bindErr) {
            $bindMsg = [string](Get-RpcField -Obj $bindErr -Name "message")
            if ($bindMsg -notmatch "already|exists|bound") {
                $bindCode = Get-RpcField -Obj $bindErr -Name "code"
                throw ("ua_bindPersona failed: code={0} message={1}" -f $bindCode, $bindMsg)
            }
        }
    }

    $sendParams = @{
        chain_id = [UInt64]$ChainId
        raw_tx = $RawTx
        require_public_broadcast = $true
        return_detail = $true
    }
    if ($UcaId) {
        $sendParams.uca_id = $UcaId
    }

    $sendReq = @{
        jsonrpc = "2.0"
        id = 2
        method = "eth_sendRawTransaction"
        params = $sendParams
    } | ConvertTo-Json -Depth 16 -Compress

    $sendResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $sendReq
    $sendErr = Get-RpcField -Obj $sendResp -Name "error"
    if ($null -ne $sendErr) {
        $summary.send_result = $sendErr
        $sendCode = Get-RpcField -Obj $sendErr -Name "code"
        $sendMsg = Get-RpcField -Obj $sendErr -Name "message"
        throw ("eth_sendRawTransaction failed: code={0} message={1}" -f $sendCode, $sendMsg)
    }
    $sendResult = Get-RpcField -Obj $sendResp -Name "result"
    $txHash = Resolve-TxHashFromResult -Result $sendResult -Context "eth_sendRawTransaction"
    $summary.tx_hash = $txHash
    $summary.send_result = $sendResult

    $statusReq = @{
        jsonrpc = "2.0"
        id = 3
        method = "evm_getTxSubmitStatus"
        params = @{
            chain_id = [UInt64]$ChainId
            tx_hash = $txHash
        }
    } | ConvertTo-Json -Depth 16 -Compress
    try {
        $statusResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $statusReq
        $statusResult = Get-RpcField -Obj $statusResp -Name "result"
        if ($statusResult) {
            $summary.submit_status = $statusResult
            $accepted = [bool](Get-RpcField -Obj $statusResult -Name "accepted")
            $pending = [bool](Get-RpcField -Obj $statusResult -Name "pending")
            if ($accepted -or $pending) {
                $summary.accepted_or_pending = $true
            }
        }
    } catch {
        # optional: keep canary flow running even if status endpoint is unavailable
    }

    if ($BroadcastMode -eq "fullnode_only") {
        $capsReq2 = @{
            jsonrpc = "2.0"
            id = 10
            method = "evm_getRuntimeProtocolCaps"
            params = @{
                chain_id = [UInt64]$ChainId
            }
        } | ConvertTo-Json -Depth 10 -Compress
        $capsResp2 = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $capsReq2
        $capsErr2 = Get-RpcField -Obj $capsResp2 -Name "error"
        if ($null -eq $capsErr2) {
            $capsResult2 = Get-RpcField -Obj $capsResp2 -Name "result"
            if ($capsResult2) {
                $summary.runtime_protocol_caps_after_send = $capsResult2
                $nativeDiscovery2 = [bool](Get-RpcField -Obj $capsResult2 -Name "native_peer_discovery")
                $nativeHandshake2 = [bool](Get-RpcField -Obj $capsResult2 -Name "native_eth_handshake")
                $nativeSync2 = [bool](Get-RpcField -Obj $capsResult2 -Name "native_snap_sync_state_machine")
                $summary.fullnode_native_ready_after_send = ($nativeDiscovery2 -and $nativeHandshake2 -and $nativeSync2)
            }
        }
    }

    $receiptReqTemplate = @{
        jsonrpc = "2.0"
        id = 4
        method = "eth_getTransactionReceipt"
        params = @{
            chain_id = [UInt64]$ChainId
            tx_hash = $txHash
        }
    }

    $requireReceipt = $true
    if ($BroadcastMode -eq "fullnode_only" -and -not $RequireReceiptInFullnode) {
        $requireReceipt = $false
    }

    if ($requireReceipt) {
        for ($i = 1; $i -le $PollMaxAttempts; $i++) {
            $summary.poll_attempts = $i
            $receiptReq = $receiptReqTemplate | ConvertTo-Json -Depth 10 -Compress
            $receiptResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $receiptReq
            $receiptErr = Get-RpcField -Obj $receiptResp -Name "error"
            if ($null -ne $receiptErr) {
                $receiptCode = Get-RpcField -Obj $receiptErr -Name "code"
                $receiptMsg = Get-RpcField -Obj $receiptErr -Name "message"
                throw ("eth_getTransactionReceipt failed: code={0} message={1}" -f $receiptCode, $receiptMsg)
            }
            $receiptResult = Get-RpcField -Obj $receiptResp -Name "result"
            if ($receiptResult) {
                $summary.receipt = $receiptResult
                $summary.receipt_found = $true
                break
            }
            Start-Sleep -Milliseconds $PollIntervalMs
        }
        if (-not $summary.receipt_found) {
            throw "receipt not found within polling window"
        }
    } else {
        if ($summary.accepted_or_pending -ne $true) {
            throw "fullnode_only canary failed: tx not observed as accepted/pending"
        }
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
    $summaryJson = $summary | ConvertTo-Json -Depth 24
    Set-Content -Path $SummaryOut -Value $summaryJson -Encoding UTF8
    Write-Host "summary written: $SummaryOut"
}

if ($BroadcastMode -eq "fullnode_only" -and -not $RequireReceiptInFullnode) {
    if ($summary.accepted_or_pending -ne $true) {
        throw "mainnet write canary failed (fullnode_only pending criteria); inspect summary: $SummaryOut"
    }
} elseif (-not $summary.receipt_found) {
    throw "mainnet write canary failed; inspect summary: $SummaryOut"
}

Write-Host ("mainnet write canary ok: tx_hash={0}" -f $summary.tx_hash)

