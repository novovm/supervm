param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$UpstreamRpc = "",
    [string]$RawTx = "",
    [UInt64]$ChainId = 1,
    [int]$PollMaxAttempts = 45,
    [int]$PollIntervalMs = 3000,
    [UInt64]$UpstreamTimeoutMs = 20000,
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
if (-not $UpstreamRpc) {
    throw "missing UpstreamRpc: pass -UpstreamRpc or set NOVOVM_GATEWAY_ETH_UPSTREAM_RPC"
}
if (-not $RawTx) {
    throw "missing RawTx: pass -RawTx or set NOVOVM_EVM_MAINNET_CANARY_RAW_TX"
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
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC" = $UpstreamRpc
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC" = $UpstreamRpc
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_TIMEOUT_MS" = ([string]$UpstreamTimeoutMs)
    "NOVOVM_GATEWAY_ETH_PUBLIC_BROADCAST_UPSTREAM_RPC_TIMEOUT_MS" = ([string]$UpstreamTimeoutMs)
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_$ChainId" = $UpstreamRpc
    "NOVOVM_GATEWAY_ETH_UPSTREAM_RPC_CHAIN_$chainHex" = $UpstreamRpc
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
    chain_id = $ChainId
    upstream_rpc = $UpstreamRpc
    tx_hash = $null
    send_result = $null
    submit_status = $null
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
    if (-not $chainResp.result) {
        throw "eth_chainId returned empty result"
    }

    $sendReq = @{
        jsonrpc = "2.0"
        id = 2
        method = "eth_sendRawTransaction"
        params = @{
            chain_id = [UInt64]$ChainId
            raw_tx = $RawTx
            require_public_broadcast = $true
            return_detail = $true
        }
    } | ConvertTo-Json -Depth 16 -Compress

    $sendResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $sendReq
    if ($sendResp.error) {
        $summary.send_result = $sendResp.error
        throw ("eth_sendRawTransaction failed: code={0} message={1}" -f $sendResp.error.code, $sendResp.error.message)
    }

    $txHash = Resolve-TxHashFromResult -Result $sendResp.result -Context "eth_sendRawTransaction"
    $summary.tx_hash = $txHash
    $summary.send_result = $sendResp.result

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
        if ($statusResp.result) {
            $summary.submit_status = $statusResp.result
        }
    } catch {
        # optional: keep canary flow running even if status endpoint is unavailable
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

    for ($i = 1; $i -le $PollMaxAttempts; $i++) {
        $summary.poll_attempts = $i
        $receiptReq = $receiptReqTemplate | ConvertTo-Json -Depth 10 -Compress
        $receiptResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $receiptReq
        if ($receiptResp.error) {
            throw ("eth_getTransactionReceipt failed: code={0} message={1}" -f $receiptResp.error.code, $receiptResp.error.message)
        }
        if ($receiptResp.result) {
            $summary.receipt = $receiptResp.result
            $summary.receipt_found = $true
            break
        }
        Start-Sleep -Milliseconds $PollIntervalMs
    }

    if (-not $summary.receipt_found) {
        throw "receipt not found within polling window"
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

if (-not $summary.receipt_found) {
    throw "mainnet write canary failed; inspect summary: $SummaryOut"
}

Write-Host ("mainnet write canary ok: tx_hash={0}" -f $summary.tx_hash)

