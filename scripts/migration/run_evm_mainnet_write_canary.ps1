param(
    [string]$RepoRoot = "",
    [string]$GatewayBind = "127.0.0.1:9899",
    [string]$UpstreamRpc = "",
    [string]$RawTx = "",
    [string]$UcaId = "",
    [string]$FromAddress = "",
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
    uca_id = $UcaId
    uca_id_effective = $UcaId
    from_address = $FromAddress
    tx_hash = $null
    send_result = $null
    send_retry_result = $null
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
        params = @{}
    }
    $sendReq.params["chain_id"] = [UInt64]$ChainId
    $sendReq.params["raw_tx"] = $RawTx
    $sendReq.params["require_public_broadcast"] = $true
    $sendReq.params["return_detail"] = $true

    if ($FromAddress) {
        if (-not $UcaId) {
            $UcaId = "uca-mainnet-write-{0:yyyyMMddHHmmssfff}" -f (Get-Date)
        }
        $summary.uca_id = $UcaId
        $summary.uca_id_effective = $UcaId

        $createReq = @{
            jsonrpc = "2.0"
            id = 11
            method = "ua_createUca"
            params = @{
                uca_id = $UcaId
            }
        } | ConvertTo-Json -Depth 10 -Compress
        $null = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $createReq

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
        $null = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $bindReq

        $sendReq.params["uca_id"] = $UcaId
        $sendReq.params["from"] = $FromAddress
    }

    $sendReq = $sendReq | ConvertTo-Json -Depth 16 -Compress

    $sendResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $sendReq
    $summary.send_result = $sendResp
    $sendError = $null
    $sendErrorProp = $sendResp.PSObject.Properties["error"]
    if ($null -ne $sendErrorProp) {
        $sendError = $sendErrorProp.Value
    }
    if ($null -ne $sendError) {
        $sendErrorCodeProp = $sendError.PSObject.Properties["code"]
        $sendErrorMessageProp = $sendError.PSObject.Properties["message"]
        $sendErrorCode = if ($null -ne $sendErrorCodeProp) { [string]$sendErrorCodeProp.Value } else { "" }
        $sendErrorMessage = if ($null -ne $sendErrorMessageProp -and $null -ne $sendErrorMessageProp.Value) { [string]$sendErrorMessageProp.Value } else { "" }
        if ($FromAddress -and $sendErrorCode -eq "-32033" -and $sendErrorMessage) {
            $m = [regex]::Match($sendErrorMessage, "binding_owner=([a-zA-Z0-9_\\-]+)")
            if ($m.Success) {
                $bindingOwner = $m.Groups[1].Value
                if (-not [string]::IsNullOrWhiteSpace($bindingOwner)) {
                    $summary.uca_id_effective = $bindingOwner
                    $retryReq = @{
                        jsonrpc = "2.0"
                        id = 13
                        method = "eth_sendRawTransaction"
                        params = @{
                            uca_id = $bindingOwner
                            chain_id = [UInt64]$ChainId
                            from = $FromAddress
                            raw_tx = $RawTx
                            require_public_broadcast = $true
                            return_detail = $true
                        }
                    } | ConvertTo-Json -Depth 16 -Compress
                    $retryResp = Invoke-RestMethod -Uri $url -Method Post -ContentType "application/json" -Body $retryReq
                    $summary.send_retry_result = $retryResp
                    $sendResp = $retryResp
                    $sendError = $null
                    $retryErrorProp = $retryResp.PSObject.Properties["error"]
                    if ($null -ne $retryErrorProp) {
                        $sendError = $retryErrorProp.Value
                    }
                }
            }
        }
    }
    if ($null -ne $sendError) {
        $sendErrorCodeProp = $sendError.PSObject.Properties["code"]
        $sendErrorMessageProp = $sendError.PSObject.Properties["message"]
        $sendErrorCode = if ($null -ne $sendErrorCodeProp) { [string]$sendErrorCodeProp.Value } else { "" }
        $sendErrorMessage = if ($null -ne $sendErrorMessageProp -and $null -ne $sendErrorMessageProp.Value) { [string]$sendErrorMessageProp.Value } else { "" }
        throw ("eth_sendRawTransaction failed: code={0} message={1}" -f $sendErrorCode, $sendErrorMessage)
    }

    $sendResult = $null
    $sendResultProp = $sendResp.PSObject.Properties["result"]
    if ($null -ne $sendResultProp) {
        $sendResult = $sendResultProp.Value
    }
    $txHash = Resolve-TxHashFromResult -Result $sendResult -Context "eth_sendRawTransaction"
    $summary.tx_hash = $txHash

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
        $statusResult = $null
        $statusResultProp = $statusResp.PSObject.Properties["result"]
        if ($null -ne $statusResultProp) {
            $statusResult = $statusResultProp.Value
        }
        if ($null -ne $statusResult) {
            $summary.submit_status = $statusResult
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
        $receiptError = $null
        $receiptErrorProp = $receiptResp.PSObject.Properties["error"]
        if ($null -ne $receiptErrorProp) {
            $receiptError = $receiptErrorProp.Value
        }
        if ($null -ne $receiptError) {
            $receiptErrorCodeProp = $receiptError.PSObject.Properties["code"]
            $receiptErrorMessageProp = $receiptError.PSObject.Properties["message"]
            $receiptErrorCode = if ($null -ne $receiptErrorCodeProp) { [string]$receiptErrorCodeProp.Value } else { "" }
            $receiptErrorMessage = if ($null -ne $receiptErrorMessageProp -and $null -ne $receiptErrorMessageProp.Value) { [string]$receiptErrorMessageProp.Value } else { "" }
            throw ("eth_getTransactionReceipt failed: code={0} message={1}" -f $receiptErrorCode, $receiptErrorMessage)
        }
        $receiptResult = $null
        $receiptResultProp = $receiptResp.PSObject.Properties["result"]
        if ($null -ne $receiptResultProp) {
            $receiptResult = $receiptResultProp.Value
        }
        if ($null -ne $receiptResult) {
            $summary.receipt = $receiptResult
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
